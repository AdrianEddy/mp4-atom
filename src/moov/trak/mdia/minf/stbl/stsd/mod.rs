mod ac3;
mod amr;
mod audio;
mod av01;
mod btrt;
mod camm;
mod ccst;
mod chnl;
mod colr;
mod eac3;
mod evte;
mod fiel;
mod flac;
mod ftab;
mod h264;
mod hevc;
mod jpeg;
mod mebx;
mod metadata;
mod mett;
mod metx;
mod mp3;
mod mp4a;
mod mp4v;
mod opus;
mod pasp;
mod pcm;
mod plaintext;
mod st3d;
mod sv3d;
mod taic;
mod tx3g;
mod uncv;
mod unknown;
mod urim;
mod vexu;
mod visual;
mod vp9;
mod wvtt;

pub use ac3::*;
pub use amr::*;
pub use audio::*;
pub use av01::*;
pub use btrt::*;
pub use camm::*;
pub use ccst::*;
pub use chnl::*;
pub use colr::*;
pub use eac3::*;
pub use evte::*;
pub use fiel::*;
pub use flac::*;
pub use ftab::*;
pub use h264::*;
pub use hevc::*;
pub use jpeg::*;
pub use mebx::*;
pub use metadata::*;
pub use mett::*;
pub use metx::*;
pub use mp3::*;
pub use mp4a::*;
pub use mp4v::*;
pub use opus::*;
pub use pasp::*;
pub use pcm::*;
pub use plaintext::*;
pub use st3d::*;
pub use sv3d::*;
pub use taic::*;
pub use tx3g::*;
pub use uncv::*;
pub use unknown::*;
pub use urim::*;
pub use vexu::*;
pub use visual::*;
pub use vp9::*;
pub use wvtt::*;

use crate::*;
use derive_more::From;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Stsd {
    pub codecs: Vec<Codec>,
}

/// Called a "sample entry" in the ISOBMFF specification.
#[derive(Debug, Clone, PartialEq, Eq, From)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Codec {
    // H264
    Avc1(Avc1),

    // H264: SPS/PPS may also be inline
    Avc3(Avc3),

    // Photo - JPEG (QuickTime Motion JPEG)
    Jpeg(Jpeg),

    // HEVC: SPS/PPS/VPS is inline
    Hev1(Hev1),

    // HEVC: SPS/PPS/VPS is in a separate atom
    Hvc1(Hvc1),

    // VP8
    Vp08(Vp08),

    // VP9
    Vp09(Vp09),

    // AV1
    Av01(Av01),

    // AAC
    Mp4a(Mp4a),

    // MP3
    Mp3(Mp3),

    // MPEG-4 Part 2 Visual
    Mp4v(Mp4v),

    // Text
    Tx3g(Tx3g),

    // Opus
    Opus(Opus),

    // Uncompressed video
    Uncv(Uncv),

    // FLAC audio
    Flac(Flac),

    // AC-3 audio
    Ac3(Ac3),

    // EAC-3 audio
    Eac3(Eac3),

    // Uncompressed audio
    // ipcm and fpcm are from 23003-5.
    Ipcm(Ipcm),
    Fpcm(Fpcm),
    // sowt / twos and in24 / in32 / fl32 / fl64 / lpcm are Quicktime (QTFF-2001).
    // s16l seems to be something that VLC produced at some point.
    Sowt(Sowt),
    Twos(Twos),
    Lpcm(Lpcm),
    In24(In24),
    In32(In32),
    Fl32(Fl32),
    Fl64(Fl64),
    S16l(S16l),

    // WebVTT, ISO/IEC 14496-30
    Wvtt(Wvtt),

    // 3GPP Narrowband audio (3GPP TS 26.244 or ETSI TS 126 244)
    Samr(Samr),

    // Text-based timed metadata
    Mett(Mett),

    // XML-based timed metadata
    Metx(Metx),

    // URI-based timed metadata
    Urim(Urim),

    // Camera motion metadata (gyroscope/accelerometer), used by Google devices
    Camm(Camm),

    // CMAF/DASH Event Message track, carrying `Emsg` boxes as samples
    // (ISO/IEC 23001-18).
    Evte(Evte),

    // Structured/"boxed" timed metadata, QuickTime "Metadata Media"
    Mebx(Mebx),

    // Unknown
    Unknown(FourCC, Vec<u8>),
}

impl Codec {
    /// The four-character code this sample entry is written under — the kind of
    /// the [`Stsd`] child box, i.e. each variant's [`Atom::KIND`], or the
    /// retained code of an entry this crate does not model.
    ///
    /// The FourCC is a track's *identity*, not a detail of its payload: an
    /// ISO-BMFF timed-metadata track is told apart from another one only by
    /// this code (`camm` gyroscope samples versus `mett` text versus a vendor's
    /// `gpmd`), so a demuxer that wants to copy a track through has to be able
    /// to ask for it.
    ///
    /// [`Codec`] is `#[non_exhaustive]`, so a downstream crate **cannot** write
    /// an exhaustive match to recover it and any table it keeps instead goes
    /// stale the next time a variant is added here — silently, because the
    /// fallback arm looks like it covers the new case. Written inside the crate
    /// that adds the variant, this cannot be forgotten.
    pub fn fourcc(&self) -> FourCC {
        match self {
            Self::Avc1(_) => Avc1::KIND,
            Self::Avc3(_) => Avc3::KIND,
            Self::Jpeg(_) => Jpeg::KIND,
            Self::Hev1(_) => Hev1::KIND,
            Self::Hvc1(_) => Hvc1::KIND,
            Self::Vp08(_) => Vp08::KIND,
            Self::Vp09(_) => Vp09::KIND,
            Self::Av01(_) => Av01::KIND,
            Self::Mp4a(_) => Mp4a::KIND,
            Self::Mp3(_) => Mp3::KIND,
            Self::Mp4v(_) => Mp4v::KIND,
            Self::Tx3g(_) => Tx3g::KIND,
            Self::Opus(_) => Opus::KIND,
            Self::Uncv(_) => Uncv::KIND,
            Self::Flac(_) => Flac::KIND,
            Self::Ac3(_) => Ac3::KIND,
            Self::Eac3(_) => Eac3::KIND,
            Self::Ipcm(_) => Ipcm::KIND,
            Self::Fpcm(_) => Fpcm::KIND,
            Self::Sowt(_) => Sowt::KIND,
            Self::Twos(_) => Twos::KIND,
            Self::Lpcm(_) => Lpcm::KIND,
            Self::In24(_) => In24::KIND,
            Self::In32(_) => In32::KIND,
            Self::Fl32(_) => Fl32::KIND,
            Self::Fl64(_) => Fl64::KIND,
            Self::S16l(_) => S16l::KIND,
            Self::Wvtt(_) => Wvtt::KIND,
            Self::Samr(_) => Samr::KIND,
            Self::Mett(_) => Mett::KIND,
            Self::Metx(_) => Metx::KIND,
            Self::Urim(_) => Urim::KIND,
            Self::Camm(_) => Camm::KIND,
            Self::Evte(_) => Evte::KIND,
            Self::Mebx(_) => Mebx::KIND,
            Self::Unknown(kind, _) => *kind,
        }
    }
}

impl Decode for Codec {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self> {
        let atom = Any::decode(buf)?;
        Ok(match atom {
            Any::Avc1(atom) => atom.into(),
            Any::Avc3(atom) => atom.into(),
            Any::Jpeg(atom) => atom.into(),
            Any::Hev1(atom) => atom.into(),
            Any::Hvc1(atom) => atom.into(),
            Any::Vp08(atom) => atom.into(),
            Any::Vp09(atom) => atom.into(),
            Any::Mp4a(atom) => atom.into(),
            Any::Mp3(atom) => atom.into(),
            Any::Mp4v(atom) => atom.into(),
            Any::Tx3g(atom) => atom.into(),
            Any::Av01(atom) => atom.into(),
            Any::Opus(atom) => atom.into(),
            Any::Uncv(atom) => atom.into(),
            Any::Flac(atom) => atom.into(),
            Any::Ac3(atom) => atom.into(),
            Any::Eac3(atom) => atom.into(),
            Any::Ipcm(atom) => atom.into(),
            Any::Fpcm(atom) => atom.into(),
            Any::Sowt(atom) => atom.into(),
            Any::Twos(atom) => atom.into(),
            Any::Lpcm(atom) => atom.into(),
            Any::In24(atom) => atom.into(),
            Any::In32(atom) => atom.into(),
            Any::Fl32(atom) => atom.into(),
            Any::Fl64(atom) => atom.into(),
            Any::S16l(atom) => atom.into(),
            Any::Wvtt(atom) => atom.into(),
            Any::Samr(atom) => atom.into(),
            Any::Mett(atom) => atom.into(),
            Any::Metx(atom) => atom.into(),
            Any::Urim(atom) => atom.into(),
            Any::Camm(atom) => atom.into(),
            Any::Evte(atom) => atom.into(),
            Any::Mebx(atom) => atom.into(),
            Any::Unknown(four_cc, body) => Self::Unknown(four_cc, body),
            unknown => {
                crate::decode_unknown(&unknown, Stsd::KIND)?;

                // The atom kind is known elsewhere in the hierarchy, but is not a supported
                // sample entry. Re-encode its body so the unknown entry remains a valid box.
                let kind = unknown.kind();
                let mut encoded = Vec::new();
                unknown.encode(&mut encoded)?;
                Self::Unknown(kind, encoded.split_off(8))
            }
        })
    }
}

impl Encode for Codec {
    fn encode<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        match self {
            // `Unknown` is `size ‖ kind ‖ body` and nothing else: `Decode` kept
            // every byte after the 8-byte header verbatim, so writing it back is
            // the header plus those bytes. Emitting it is what makes an entry
            // this crate does not model survive a read → write cycle — without
            // it a remux either drops such a track or has to splice the box in
            // afterwards, and both lose the entry's own fields (a `tmcd`'s
            // timescale and reel name, a `mebx`'s key table).
            Self::Unknown(kind, body) => buf.encode_atom(*kind, |buf| body.encode(buf)),
            Self::Avc1(atom) => atom.encode(buf),
            Self::Avc3(atom) => atom.encode(buf),
            Self::Jpeg(atom) => atom.encode(buf),
            Self::Hev1(atom) => atom.encode(buf),
            Self::Hvc1(atom) => atom.encode(buf),
            Self::Vp08(atom) => atom.encode(buf),
            Self::Vp09(atom) => atom.encode(buf),
            Self::Mp4a(atom) => atom.encode(buf),
            Self::Mp3(atom) => atom.encode(buf),
            Self::Mp4v(atom) => atom.encode(buf),
            Self::Tx3g(atom) => atom.encode(buf),
            Self::Av01(atom) => atom.encode(buf),
            Self::Opus(atom) => atom.encode(buf),
            Self::Uncv(atom) => atom.encode(buf),
            Self::Flac(atom) => atom.encode(buf),
            Self::Ac3(atom) => atom.encode(buf),
            Self::Eac3(atom) => atom.encode(buf),
            Self::Ipcm(atom) => atom.encode(buf),
            Self::Fpcm(atom) => atom.encode(buf),
            Self::Sowt(atom) => atom.encode(buf),
            Self::Twos(atom) => atom.encode(buf),
            Self::Lpcm(atom) => atom.encode(buf),
            Self::In24(atom) => atom.encode(buf),
            Self::In32(atom) => atom.encode(buf),
            Self::Fl32(atom) => atom.encode(buf),
            Self::Fl64(atom) => atom.encode(buf),
            Self::S16l(atom) => atom.encode(buf),
            Self::Wvtt(atom) => atom.encode(buf),
            Self::Samr(atom) => atom.encode(buf),
            Self::Mett(atom) => atom.encode(buf),
            Self::Metx(atom) => atom.encode(buf),
            Self::Urim(atom) => atom.encode(buf),
            Self::Camm(atom) => atom.encode(buf),
            Self::Evte(atom) => atom.encode(buf),
            Self::Mebx(atom) => atom.encode(buf),
        }
    }
}

/// The four-character codes of the boxes sitting in `bytes`, for the warning
/// [`Stsd`] logs when it drains content past its declared entry count. A
/// sibling `sv3d` or `st3d` misplaced there is exactly the sort of thing worth
/// naming, and a byte count alone does not say what was thrown away. Walking
/// stops at the first length that cannot be a box: this is diagnostics, so a
/// partial answer beats an error.
pub(crate) fn trailing_kinds(mut bytes: &[u8]) -> Vec<FourCC> {
    let mut kinds = Vec::new();
    while bytes.len() >= 8 {
        let size = u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as usize;
        kinds.push(u32::from_be_bytes(bytes[4..8].try_into().unwrap()).into());
        if size < 8 || size > bytes.len() {
            break;
        }
        bytes = &bytes[size..];
    }
    kinds
}

impl AtomExt for Stsd {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"stsd");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        let codec_count = u32::decode(buf)?;
        let mut codecs = Vec::new();

        for _ in 0..codec_count {
            let codec = Codec::decode(buf)?;
            codecs.push(codec);
        }

        // Content after the declared entries.
        //
        // `entry_count` is the box's own statement of how many sample entries
        // it holds, so anything past the last one is not a sample entry — and
        // real writers do emit some. GoPro's CineForm MOV muxer writes the
        // track's `colr` as a SIBLING of the sample entry instead of a child,
        // leaving 20 bytes here that `entry_count = 1` does not describe; that
        // is a whole box, so `skip_trailing_padding`'s sub-header contract does
        // not reach it.
        //
        // Refusing it failed the whole `moov` — and therefore the whole file —
        // over a box the ISO layout says is not there. libavformat reads
        // exactly `entries` entries and skips the rest of the box without
        // looking (`mov_read_stsd`), so draining is both what every other
        // demuxer does and what keeps two readers of the same file agreeing
        // about it. Announced rather than silent: a remainder is a malformed
        // `stsd`, it is simply not a fatal one.
        skip_trailing_padding(buf);
        let remaining = buf.remaining();
        if remaining > 0 {
            tracing::warn!(
                bytes = remaining,
                entries = codec_count,
                boxes = ?trailing_kinds(buf.slice(remaining)),
                "stsd: draining trailing content after the declared sample entries"
            );
            buf.advance(remaining);
        }

        Ok(Stsd { codecs })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        (self.codecs.len() as u32).encode(buf)?;
        for codec in &self.codecs {
            codec.encode(buf)?;
        }

        Ok(())
    }
}

/// Skip a child that a box dispatching its children by FourCC does not model:
/// `free`/`skip` padding silently, anything else through `decode_unknown`.
///
/// The child is not decoded through `Any`: `sv3d` and `vexu` both contain an
/// unrelated `proj`, so a FourCC means something else inside them.
fn skip_child<B: Buf>(header: &Header, buf: &mut B, parent: FourCC) -> Result<()> {
    let size = header.size.unwrap_or(buf.remaining());
    if size > buf.remaining() {
        return Err(Error::OutOfBounds);
    }
    if header.kind != Free::KIND && header.kind != Skip::KIND {
        let body = Vec::decode(&mut buf.slice(size))?;
        crate::decode_unknown(&Any::Unknown(header.kind, body), parent)?;
    }
    buf.advance(size);
    Ok(())
}

mod tests {
    #![cfg(not(feature = "strict"))]

    #[test]
    fn unknown_codec_parses() {
        use crate::{Codec, Decode, FourCC, Stsd};

        let input = b"\0\0\0\x1cstsd\0\0\0\0\0\0\0\x01\0\0\0\x0cdvh9\x01\x02\x03\x04";
        let mut buf = input.as_slice();

        let stsd = Stsd::decode(&mut buf).unwrap();
        assert_eq!(
            stsd.codecs,
            vec![Codec::Unknown(FourCC::new(b"dvh9"), vec![1, 2, 3, 4])]
        );
    }

    #[test]
    fn unknown_codec_round_trips_byte_for_byte() {
        use crate::{Decode, Encode, Stsd};

        let input = b"\0\0\0\x1cstsd\0\0\0\0\0\0\0\x01\0\0\0\x0cdvh9\x01\x02\x03\x04";
        let stsd = Stsd::decode(&mut input.as_slice()).unwrap();

        // `Unknown` is `size ‖ kind ‖ body`, so re-encoding the whole `stsd`
        // reproduces the input exactly — an entry this crate does not model
        // survives a read → write cycle instead of aborting it.
        let mut output = Vec::new();
        stsd.encode(&mut output).unwrap();
        assert_eq!(output, input);
    }
}

#[cfg(test)]
mod fourcc_tests {
    #[test]
    fn every_codec_names_its_own_fourcc() {
        use crate::{Codec, Decode, FourCC, MetaData, Mett, Stsd};

        // A modelled entry answers its `Atom::KIND`…
        let mett = Codec::Mett(Mett {
            metadata: MetaData {
                data_reference_index: 1,
            },
            content_encoding: String::new(),
            mime_format: "text/plain".into(),
            btrt: None,
            txtc: None,
        });
        assert_eq!(mett.fourcc(), FourCC::new(b"mett"));

        // …and an unmodelled one answers the code it was read under, so the two
        // cannot be told apart by the caller that only wants the identity.
        let input = b"\0\0\0\x1cstsd\0\0\0\0\0\0\0\x01\0\0\0\x0cdvh9\x01\x02\x03\x04";
        let stsd = Stsd::decode(&mut input.as_slice()).unwrap();
        assert_eq!(stsd.codecs[0].fourcc(), FourCC::new(b"dvh9"));

        // The code it answers is the one it writes.
        for codec in [mett, stsd.codecs[0].clone()] {
            let mut bytes = Vec::new();
            crate::Encode::encode(&codec, &mut bytes).unwrap();
            assert_eq!(
                FourCC::new(&bytes[4..8].try_into().unwrap()),
                codec.fourcc()
            );
        }
    }
}
