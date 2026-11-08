use crate::*;

// We're trying not to pollute the global namespace
pub mod esds;
pub use esds::Esds;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mp4a {
    pub audio: Audio,
    /// QuickTime version-2 sound sample description fields, when the entry was
    /// written as a v2 `AudioSampleEntry`: the real rate and channel count of a
    /// track the base [`Audio`] fields cannot state (a rate above 65535 Hz), which
    /// then hold placeholders — see [`SoundV2`].
    pub sound_v2: Option<SoundV2>,
    pub esds: Esds,
    /// The spatial-audio box: the channels are an ambisonic sound field.
    pub sa3d: Option<Sa3d>,
    pub btrt: Option<Btrt>,
    pub taic: Option<Taic>,
}

impl Atom for Mp4a {
    const KIND: FourCC = FourCC::new(b"mp4a");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let (audio, sound_v2) = Audio::decode_with_v2(buf)?;

        let mut btrt = None;
        let mut esds = None;
        let mut sa3d = None;
        let mut taic = None;

        // The `esds` sits directly under `mp4a` (ISO-BMFF) or nested inside a
        // QuickTime `wave` (siDecompressionParam) box (QTFF) — FFmpeg and many
        // camera muxers emit the latter for AAC-in-MOV.
        while let Some(atom) = Any::decode_maybe(buf)? {
            match atom {
                Any::Btrt(atom) => btrt = atom.into(),
                Any::Esds(atom) => esds = atom.into(),
                Any::Sa3d(atom) => sa3d = atom.into(),
                Any::Taic(atom) => taic = atom.into(),
                Any::Unknown(kind, body) if kind == WAVE => {
                    decode_wave(&body, &mut esds, &mut btrt)?;
                }
                unknown => Self::decode_unknown(&unknown)?,
            }
        }
        skip_trailing_padding(buf);

        Ok(Mp4a {
            audio,
            sound_v2,
            esds: esds.ok_or(Error::MissingBox(Esds::KIND))?,
            sa3d,
            btrt,
            taic,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.audio.encode_with_v2(self.sound_v2.as_ref(), buf)?;
        self.esds.encode(buf)?;
        self.sa3d.encode(buf)?;
        self.btrt.encode(buf)?;
        self.taic.encode(buf)?;

        Ok(())
    }
}

/// QuickTime `wave` (siDecompressionParam) container FourCC.
const WAVE: FourCC = FourCC::new(b"wave");

/// Scan a QuickTime `wave` box body for the `esds` (and `btrt`) it wraps.
///
/// Every other child — the `frma` format box, the nested `mp4a` terminator
/// stub, the trailing all-zero box — is skipped by size. Those are deliberately
/// NOT typed-decoded: the nested `mp4a` in particular is a 4-byte stub that is
/// not a valid audio sample entry, so decoding it as one would fail.
fn decode_wave(mut wave: &[u8], esds: &mut Option<Esds>, btrt: &mut Option<Btrt>) -> Result<()> {
    while let Some(header) = Header::decode_maybe(&mut wave)? {
        // A size-to-end (`None`) or over-long child terminates the scan — the
        // esds, if present, precedes any such terminator.
        let Some(size) = header.size.and(header.size_within(wave.remaining())) else {
            break;
        };
        let mut child = wave.slice(size);
        if header.kind == Esds::KIND {
            *esds = Some(Esds::decode_body(&mut child)?);
        } else if header.kind == Btrt::KIND {
            *btrt = Some(Btrt::decode_body(&mut child)?);
        }
        wave.advance(size);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mp4a() {
        let expected = Mp4a {
            audio: Audio {
                data_reference_index: 1,
                channel_count: 2,
                sample_size: 16,
                sample_rate: 48000.into(),
            },
            sound_v2: None,
            esds: Esds {
                es_desc: esds::EsDescriptor {
                    es_id: 2,
                    dec_config: esds::DecoderConfig {
                        object_type_indication: 0x40,
                        stream_type: 0x05,
                        up_stream: 0,
                        buffer_size_db: Default::default(),
                        max_bitrate: 67695,
                        avg_bitrate: 67695,
                        dec_specific: Some(esds::DecoderSpecific {
                            profile: 2,
                            freq_index: 4,
                            chan_conf: 2,
                            raw: vec![0x12, 0x10],
                        }),
                    },
                    sl_config: esds::SLConfig::default(),
                },
            },
            btrt: Some(Btrt {
                buffer_size_db: 1,
                max_bitrate: 2,
                avg_bitrate: 3,
            }),
            taic: None,
            sa3d: None,
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let mut buf = buf.as_ref();
        let decoded = Mp4a::decode(&mut buf).unwrap();
        assert_eq!(decoded, expected);
    }

    // Build `[size:u32][fourcc][body]`.
    fn atom_box(fourcc: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(8 + body.len());
        v.extend_from_slice(&((8 + body.len()) as u32).to_be_bytes());
        v.extend_from_slice(fourcc);
        v.extend_from_slice(body);
        v
    }

    // QuickTime (QTFF) `mp4a` sample entry: the AAC `esds` is nested inside a
    // `wave` (siDecompressionParam) box rather than a direct child of `mp4a`.
    // FFmpeg and camera muxers (MotionCam, etc.) emit MOV audio this way.
    #[test]
    fn test_mp4a_quicktime_wave() {
        let audio = Audio {
            data_reference_index: 1,
            channel_count: 2,
            sample_size: 16,
            sample_rate: 48000.into(),
        };
        let esds = Esds {
            es_desc: esds::EsDescriptor {
                es_id: 2,
                dec_config: esds::DecoderConfig {
                    object_type_indication: 0x40,
                    stream_type: 0x05,
                    up_stream: 0,
                    buffer_size_db: Default::default(),
                    max_bitrate: 67695,
                    avg_bitrate: 67695,
                    dec_specific: Some(esds::DecoderSpecific {
                        profile: 2,
                        freq_index: 4,
                        chan_conf: 2,
                        raw: vec![0x12, 0x10],
                    }),
                },
                sl_config: esds::SLConfig::default(),
            },
        };

        let btrt = Btrt {
            buffer_size_db: 1,
            max_bitrate: 2,
            avg_bitrate: 3,
        };

        // Encode the pieces the QuickTime layout wraps.
        let mut audio_bytes = Vec::new();
        audio.encode(&mut audio_bytes).unwrap();
        let mut esds_box = Vec::new();
        esds.encode(&mut esds_box).unwrap();
        let mut btrt_box = Vec::new();
        btrt.encode(&mut btrt_box).unwrap();

        // wave { frma('mp4a'), mp4a stub (4 zero bytes — NOT a sample entry),
        //        esds, btrt, all-zero terminator }
        let mut wave_body = Vec::new();
        wave_body.extend_from_slice(&atom_box(b"frma", b"mp4a"));
        wave_body.extend_from_slice(&atom_box(b"mp4a", &[0, 0, 0, 0]));
        wave_body.extend_from_slice(&esds_box);
        wave_body.extend_from_slice(&btrt_box);
        wave_body.extend_from_slice(&[0, 0, 0, 8, 0, 0, 0, 0]);

        // mp4a { audio-header, wave }
        let mut mp4a_body = audio_bytes;
        mp4a_body.extend_from_slice(&atom_box(b"wave", &wave_body));
        let mp4a = atom_box(b"mp4a", &mp4a_body);

        let mut buf = mp4a.as_slice();
        let decoded = Mp4a::decode(&mut buf).unwrap();
        assert_eq!(decoded.audio, audio);
        assert_eq!(decoded.esds, esds);
        // A `btrt` nested alongside the esds inside `wave` is hoisted too.
        assert_eq!(decoded.btrt, Some(btrt));
    }

    // FFmpeg's AAC-LC `esds` for a 96 kHz mono track (`freq_index` 0).
    fn esds_96k_mono() -> Esds {
        Esds {
            es_desc: esds::EsDescriptor {
                es_id: 1,
                dec_config: esds::DecoderConfig {
                    object_type_indication: 0x40,
                    stream_type: 0x05,
                    up_stream: 0,
                    buffer_size_db: Default::default(),
                    max_bitrate: 68963,
                    avg_bitrate: 68963,
                    dec_specific: Some(esds::DecoderSpecific {
                        profile: 2,
                        freq_index: 0,
                        chan_conf: 1,
                        raw: vec![0x10, 0x08],
                    }),
                },
                sl_config: esds::SLConfig::default(),
            },
        }
    }

    // The QuickTime version-2 sound description FFmpeg writes for AAC in a MOV
    // above 65535 Hz: the base fields hold QTFF's placeholders (three channels,
    // a rate of 1.0) and the real rate and channel count live in the v2 fields.
    fn audio_v2_placeholders() -> (Audio, SoundV2) {
        (
            Audio {
                data_reference_index: 1,
                channel_count: 3,
                sample_size: 16,
                sample_rate: FixedPoint::new(1, 0),
            },
            SoundV2 {
                sample_rate: 96000,
                channel_count: 1,
                bits_per_channel: 0,
                format_flags: 0,
                bytes_per_packet: 0,
                lpcm_frames_per_packet: 0,
            },
        )
    }

    // A version-2 entry keeps its v2 fields through a decode → encode → decode
    // round trip; it decoded as the placeholders before.
    #[test]
    fn test_mp4a_sound_v2() {
        let (audio, sound_v2) = audio_v2_placeholders();
        let expected = Mp4a {
            audio,
            sound_v2: Some(sound_v2),
            esds: esds_96k_mono(),
            btrt: None,
            taic: None,
            sa3d: None,
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let decoded = Mp4a::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(decoded, expected);
        assert_eq!(decoded.sound_v2.unwrap().sample_rate, 96000);
    }

    // FFmpeg's layout for AAC at 96 kHz in a MOV: a version-2 entry whose `esds`
    // sits in a `wave`.
    #[test]
    fn test_mp4a_sound_v2_quicktime_wave() {
        let (audio, sound_v2) = audio_v2_placeholders();
        let mut entry = Vec::new();
        audio.encode_with_v2(Some(&sound_v2), &mut entry).unwrap();
        assert_eq!(entry.len(), 64);
        assert_eq!(entry[8..10], 2u16.to_be_bytes(), "version 2");

        let mut esds_box = Vec::new();
        esds_96k_mono().encode(&mut esds_box).unwrap();
        let mut wave_body = Vec::new();
        wave_body.extend_from_slice(&atom_box(b"frma", b"mp4a"));
        wave_body.extend_from_slice(&atom_box(b"mp4a", &[0, 0, 0, 0]));
        wave_body.extend_from_slice(&esds_box);
        wave_body.extend_from_slice(&[0, 0, 0, 8, 0, 0, 0, 0]);
        entry.extend_from_slice(&atom_box(b"wave", &wave_body));
        let mp4a = atom_box(b"mp4a", &entry);

        let decoded = Mp4a::decode(&mut mp4a.as_slice()).unwrap();
        assert_eq!(decoded.audio, audio);
        assert_eq!(decoded.sound_v2, Some(sound_v2));
        assert_eq!(decoded.esds, esds_96k_mono());
    }

    #[test]
    fn test_mp4a_with_taic() {
        let expected = Mp4a {
            audio: Audio {
                data_reference_index: 1,
                channel_count: 2,
                sample_size: 16,
                sample_rate: 48000.into(),
            },
            sound_v2: None,
            esds: Esds {
                es_desc: esds::EsDescriptor {
                    es_id: 2,
                    dec_config: esds::DecoderConfig {
                        object_type_indication: 0x40,
                        stream_type: 0x05,
                        up_stream: 0,
                        buffer_size_db: Default::default(),
                        max_bitrate: 67695,
                        avg_bitrate: 67695,
                        dec_specific: Some(esds::DecoderSpecific {
                            profile: 2,
                            freq_index: 4,
                            chan_conf: 2,
                            raw: vec![0x12, 0x10],
                        }),
                    },
                    sl_config: esds::SLConfig::default(),
                },
            },
            btrt: None,
            taic: Some(Taic {
                time_uncertainty: 1,
                clock_resolution: 2,
                clock_drift_rate: 4,
                clock_type: ClockType::CanSync,
            }),
            sa3d: None,
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let mut buf = buf.as_ref();
        let decoded = Mp4a::decode(&mut buf).unwrap();
        assert_eq!(decoded, expected);
    }

    /// A first-order sound field: the `SA3D` box follows the `esds`, as
    /// libavformat's `mov_write_audio_tag` writes it, and both survive a decode
    /// → encode → decode round trip.
    #[test]
    fn test_mp4a_with_sa3d() {
        let expected = Mp4a {
            audio: Audio {
                data_reference_index: 1,
                channel_count: 4,
                sample_size: 16,
                sample_rate: 48000.into(),
            },
            sound_v2: None,
            esds: esds_96k_mono(),
            sa3d: Some(Sa3d {
                ambisonic_order: 1,
                channel_map: vec![0, 1, 2, 3],
                ..Default::default()
            }),
            btrt: None,
            taic: None,
        };

        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();
        let esds_at = buf.windows(4).position(|w| w == b"esds").unwrap();
        let sa3d_at = buf.windows(4).position(|w| w == b"SA3D").unwrap();
        assert!(sa3d_at > esds_at, "SA3D follows the esds");

        let decoded = Mp4a::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(decoded, expected);
    }
}
