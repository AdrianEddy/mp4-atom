use crate::coding::{Decode, Encode};
use crate::{Buf, BufMut, Error, FixedPoint, Result};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audio {
    pub data_reference_index: u16,
    pub channel_count: u16,
    pub sample_size: u16,
    pub sample_rate: FixedPoint<u16>,
}

/// The QuickTime **version-2** sound sample description fields (QTFF).
///
/// A version-2 `AudioSampleEntry` (which `lpcm` mandates, and which carries
/// rates/channel-counts the legacy fields cannot represent) sets the base
/// [`Audio`] `channel_count` / `sample_size` / `sample_rate` to spec-mandated
/// placeholders; the real values live here, alongside the CoreAudio LPCM
/// `format_flags` that name the storage format (float vs int, endianness).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SoundV2 {
    /// `audioSampleRate` — the real rate (the on-disk `f64` rounded to an
    /// integer; LPCM rates are integral).
    pub sample_rate: u32,
    /// `numAudioChannels` — the real channel count.
    pub channel_count: u32,
    /// `constBitsPerChannel` — bits per sample.
    pub bits_per_channel: u32,
    /// `formatSpecificFlags` — CoreAudio LPCM flags (bit 0 = float, bit 1 =
    /// big-endian, bit 2 = signed integer, …).
    pub format_flags: u32,
    /// `constBytesPerAudioPacket` — the size of one packet in bytes, or `0`
    /// when it varies from packet to packet.
    ///
    /// Carried rather than recomputed: `channel_count * bits_per_channel / 8`
    /// is only the right answer for uncompressed one-frame-per-packet PCM, and
    /// deriving it from two fields read verbatim off the wire is arithmetic on
    /// attacker-chosen `u32`s.
    pub bytes_per_packet: u32,
    /// `constLPCMFramesPerAudioPacket` — the number of PCM frames one packet
    /// carries (`1` for LPCM), or `0` when it varies.
    pub lpcm_frames_per_packet: u32,
}

impl Audio {
    /// Decode the `AudioSampleEntry` base plus any QuickTime v1/v2 sound sample
    /// description extension. The v1 extension carries no field a consumer needs
    /// (it is skipped); the v2 extension's real rate / channels / format is
    /// returned as [`SoundV2`]. Plain [`Decode`] discards the extension.
    ///
    /// Every version-2 field except the constant `sizeOfStructOnly` is kept, so
    /// a `decode`→`encode` round trip of a version-2 entry is byte-for-byte.
    pub fn decode_with_v2<B: Buf>(buf: &mut B) -> Result<(Self, Option<SoundV2>)> {
        u32::decode(buf)?; // reserved
        u16::decode(buf)?; // reserved
        let data_reference_index = u16::decode(buf)?;
        let version = u16::decode(buf)?;
        u16::decode(buf)?; // reserved
        u32::decode(buf)?; // reserved
        let channel_count = u16::decode(buf)?;
        let sample_size = u16::decode(buf)?;
        u32::decode(buf)?; // pre-defined, reserved
        let sample_rate = FixedPoint::decode(buf)?;

        let v2 = match version {
            0 => None,
            1 => {
                // QuickTime sound sample description version 1.
                u64::decode(buf)?;
                u64::decode(buf)?;
                None
            }
            2 => {
                // QuickTime sound sample description version 2.
                u32::decode(buf)?; // sizeOfStructOnly
                let sample_rate = f64::from_bits(u64::decode(buf)?).round() as u32;
                let channel_count = u32::decode(buf)?;
                u32::decode(buf)?; // always7F000000
                let bits_per_channel = u32::decode(buf)?;
                let format_flags = u32::decode(buf)?;
                let bytes_per_packet = u32::decode(buf)?; // constBytesPerAudioPacket
                let lpcm_frames_per_packet = u32::decode(buf)?; // constLPCMFramesPerAudioPacket
                Some(SoundV2 {
                    sample_rate,
                    channel_count,
                    bits_per_channel,
                    format_flags,
                    bytes_per_packet,
                    lpcm_frames_per_packet,
                })
            }
            n => return Err(Error::UnknownQuicktimeVersion(n)),
        };

        Ok((
            Self {
                data_reference_index,
                channel_count,
                sample_size,
                sample_rate,
            },
            v2,
        ))
    }

    /// Encode the `AudioSampleEntry` base, emitting the version-2 sound sample
    /// description extension when `v2` is `Some` (the base fields then carry the
    /// QTFF placeholders and `version` becomes 2). `None` writes the version-0
    /// form — the shared [`Encode`] path.
    ///
    /// Every version-2 field is written from [`SoundV2`] as it was read; only
    /// `sizeOfStructOnly`, which QTFF fixes at 72 for the version-2 struct, is
    /// emitted as the constant.
    pub fn encode_with_v2<B: BufMut>(&self, v2: Option<&SoundV2>, buf: &mut B) -> Result<()> {
        0u32.encode(buf)?; // reserved
        0u16.encode(buf)?; // reserved
        self.data_reference_index.encode(buf)?;
        (if v2.is_some() { 2u16 } else { 0u16 }).encode(buf)?; // version
        0u16.encode(buf)?; // reserved
        0u32.encode(buf)?; // reserved
        self.channel_count.encode(buf)?;
        self.sample_size.encode(buf)?;
        0u32.encode(buf)?; // reserved
        self.sample_rate.encode(buf)?;

        if let Some(v2) = v2 {
            72u32.encode(buf)?; // sizeOfStructOnly (constant)
            (v2.sample_rate as f64).to_bits().encode(buf)?; // audioSampleRate
            v2.channel_count.encode(buf)?;
            0x7F00_0000u32.encode(buf)?; // always7F000000
            v2.bits_per_channel.encode(buf)?;
            v2.format_flags.encode(buf)?;
            v2.bytes_per_packet.encode(buf)?; // constBytesPerAudioPacket
            v2.lpcm_frames_per_packet.encode(buf)?; // constLPCMFramesPerAudioPacket
        }

        Ok(())
    }
}

impl Encode for Audio {
    fn encode<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.encode_with_v2(None, buf)
    }
}
impl Decode for Audio {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self> {
        Ok(Self::decode_with_v2(buf)?.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Stsd;

    // A QuickTime **version-1** sound sample description: `decode_with_v2` must
    // consume the 16-byte v1 extension (samples/bytes-per-packet/frame/sample)
    // and discard it — returning the base fields with NO `SoundV2` (that is the
    // version-2 path). Distinct from `pcm::tests::test_pcm_v1_chnl`, which
    // exercises the PCM `pcmC`/`chnl` child boxes, not the sound-description
    // version.
    #[test]
    fn test_audio_v1_extension_is_skipped() {
        let bytes: &[u8] = &[
            0, 0, 0, 0, // reserved
            0, 0, // reserved
            0, 1, // data_reference_index
            0, 1, // version = 1
            0, 0, // reserved
            0, 0, 0, 0, // reserved
            0, 2, // channel_count
            0, 16, // sample_size
            0, 0, 0, 0, // pre-defined + reserved
            0xbb, 0x80, 0, 0, // sample_rate = 48000 << 16
            // version-1 extension (16 bytes) — consumed and discarded.
            0, 0, 4, 0, 0, 0, 0, 1, 0, 0, 0, 4, 0, 0, 0, 2,
        ];

        let (audio, v2) = Audio::decode_with_v2(&mut &bytes[..]).expect("version-1 entry decodes");
        assert!(v2.is_none(), "a version-1 entry yields no SoundV2");
        assert_eq!(audio.data_reference_index, 1);
        assert_eq!(audio.channel_count, 2);
        assert_eq!(audio.sample_size, 16);
        assert_eq!(audio.sample_rate.integer(), 48000);
    }

    /// The 64-byte body of a version-2 `AudioSampleEntry`: the 28-byte base with
    /// the QTFF placeholders, then the 36-byte version-2 sound description with
    /// the four fields the caller chooses.
    fn sound_v2_body(
        channel_count: u32,
        bits_per_channel: u32,
        bytes_per_packet: u32,
        frames_per_packet: u32,
    ) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&[0, 0, 0, 0]); // reserved
        b.extend_from_slice(&[0, 0]); // reserved
        b.extend_from_slice(&1u16.to_be_bytes()); // data_reference_index
        b.extend_from_slice(&2u16.to_be_bytes()); // version = 2
        b.extend_from_slice(&[0, 0]); // reserved
        b.extend_from_slice(&[0, 0, 0, 0]); // reserved
        b.extend_from_slice(&3u16.to_be_bytes()); // channel_count placeholder
        b.extend_from_slice(&16u16.to_be_bytes()); // sample_size placeholder
        b.extend_from_slice(&[0, 0, 0, 0]); // pre-defined + reserved
        b.extend_from_slice(&(1u32 << 16).to_be_bytes()); // sample_rate placeholder 1.0
        b.extend_from_slice(&72u32.to_be_bytes()); // sizeOfStructOnly
        b.extend_from_slice(&48000f64.to_bits().to_be_bytes()); // audioSampleRate
        b.extend_from_slice(&channel_count.to_be_bytes()); // numAudioChannels
        b.extend_from_slice(&0x7F00_0000u32.to_be_bytes()); // always7F000000
        b.extend_from_slice(&bits_per_channel.to_be_bytes()); // constBitsPerChannel
        b.extend_from_slice(&0u32.to_be_bytes()); // formatSpecificFlags
        b.extend_from_slice(&bytes_per_packet.to_be_bytes()); // constBytesPerAudioPacket
        b.extend_from_slice(&frames_per_packet.to_be_bytes()); // constLPCMFramesPerAudioPacket
        assert_eq!(b.len(), 64);
        b
    }

    // `numAudioChannels` and `constBitsPerChannel` are read verbatim off the
    // wire, so a file may state any pair of `u32`s. `constBytesPerAudioPacket`
    // used to be thrown away on decode and recomputed as
    // `channel_count * (bits_per_channel / 8)` on encode: an overflow panic with
    // overflow checks on, and a silently wrapped — corrupt — field without them.
    // The value is now carried, so there is no product to overflow and the entry
    // re-encodes exactly as it arrived.
    #[test]
    fn test_audio_v2_hostile_packet_fields_round_trip() {
        // 0x4000_0000 * (32 / 8) is the smallest overflowing product; the
        // all-ones channel count is the reviewer's original repro.
        for channels in [0x4000_0000u32, 0xFFFF_FFFF] {
            let body = sound_v2_body(channels, 32, 0x8000_0000, 7);

            let (audio, v2) =
                Audio::decode_with_v2(&mut &body[..]).expect("version-2 entry decodes");
            let v2 = v2.expect("a version-2 entry yields a SoundV2");
            assert_eq!(v2.channel_count, channels);
            assert_eq!(v2.bits_per_channel, 32);
            assert_eq!(v2.sample_rate, 48000);
            // Carried, not derived: neither field is `channels * bits / 8` or 1.
            assert_eq!(v2.bytes_per_packet, 0x8000_0000);
            assert_eq!(v2.lpcm_frames_per_packet, 7);

            let mut out = Vec::new();
            audio
                .encode_with_v2(Some(&v2), &mut out)
                .expect("re-encoding must not panic or fail");
            assert_eq!(out, body, "a version-2 entry round-trips byte-for-byte");
        }
    }

    // The same bytes through the container path a remux takes: `Stsd::decode`
    // then `Stsd::encode`, which is where the overflow was reproduced.
    #[test]
    fn test_stsd_lpcm_v2_hostile_packet_fields_round_trip() {
        let body = sound_v2_body(0xFFFF_FFFF, 32, 0, 0);
        let mut lpcm = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        lpcm.extend_from_slice(b"lpcm");
        lpcm.extend_from_slice(&body);
        assert_eq!(lpcm.len(), 72);

        let mut stsd_body = vec![0, 0, 0, 0]; // version + flags
        stsd_body.extend_from_slice(&1u32.to_be_bytes()); // entry_count
        stsd_body.extend_from_slice(&lpcm);
        let mut stsd = ((stsd_body.len() + 8) as u32).to_be_bytes().to_vec();
        stsd.extend_from_slice(b"stsd");
        stsd.extend_from_slice(&stsd_body);

        let decoded = Stsd::decode(&mut stsd.as_slice()).expect("stsd decodes");
        let mut out = Vec::new();
        decoded
            .encode(&mut out)
            .expect("re-encoding must not panic or fail");
        assert_eq!(out, stsd);
    }
}
