use crate::*;

ext! {
    name: Elst,
    versions: [0, 1],
    flags: {}
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Elst {
    pub entries: Vec<ElstEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ElstEntry {
    pub segment_duration: u64,
    /// Start time within the media of this edit, in media-timescale units
    /// (composition time). Signed on the wire (ISO/IEC 14496-12 §8.6.6): `None`
    /// is the `-1` "empty edit" sentinel -- a dwell with no media, used to signal
    /// an initial presentation offset -- and `Some(t)` is the media time exactly
    /// as written.
    ///
    /// `Some` is signed because `-1` is the only negative the spec defines and
    /// files carry others anyway. `libavformat` refuses those only under
    /// `FF_COMPLIANCE_STRICT` (`mov_read_elst`) and otherwise keeps the value,
    /// treating it as an ordinary edit starting before the media -- NOT as an
    /// empty edit, which is `-1` and nothing else. Refusing it here would fail
    /// the whole `moov`, so a file ffmpeg opens would not open through this
    /// crate. The decoder never yields `Some(-1)`; encoding it writes the same
    /// bytes as `None`.
    pub media_time: Option<i64>,
    // Signed 16.16 fixed-point playback rate (int(16).int(16) per §8.6.6). Always
    // present -- there is no "absent" sentinel; media_rate == 0 is a meaningful value
    // (a dwell, i.e. a frozen frame) rather than absence -- so it is not optional.
    pub media_rate: FixedPoint<i16>,
}

impl AtomExt for Elst {
    type Ext = ElstExt;

    const KIND_EXT: FourCC = FourCC::new(b"elst");

    fn decode_body_ext<B: Buf>(buf: &mut B, ext: ElstExt) -> Result<Self> {
        let entry_count = u32::decode(buf)?;

        let mut entries = Vec::new();
        for _ in 0..entry_count {
            // media_time is signed; decode it as i32/i64 so the -1 empty-edit
            // sentinel sign-extends instead of becoming +4294967295.
            let (segment_duration, media_time) = match ext.version {
                ElstVersion::V1 => (u64::decode(buf)?, i64::decode(buf)?),
                ElstVersion::V0 => (u32::decode(buf)? as u64, i32::decode(buf)? as i64),
            };

            // -1 is the empty edit; every other value, negative or not, is a
            // media time carried as written.
            let media_time = match media_time {
                -1 => None,
                t => Some(t),
            };

            entries.push(ElstEntry {
                segment_duration,
                media_time,
                media_rate: FixedPoint::decode(buf)?,
            });
        }

        Ok(Elst { entries })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<ElstExt> {
        // On the wire media_time is signed: None is the -1 empty edit.
        fn wire_media_time(media_time: Option<i64>) -> i64 {
            media_time.unwrap_or(-1)
        }

        // Prefer version 0 (32-bit) when every value fits: it matches what muxers
        // typically emit (so a V0 source round-trips byte-for-byte) and keeps the box
        // compact. segment_duration is unsigned int(32) but media_time is signed
        // int(32), so their ceilings differ.
        let use_v0 = !self.entries.iter().any(|e| {
            e.segment_duration > u32::MAX as u64
                || e.media_time
                    .is_some_and(|t| t > i32::MAX as i64 || t < i32::MIN as i64)
        });

        (self.entries.len() as u32).encode(buf)?;

        for entry in &self.entries {
            let media_time = wire_media_time(entry.media_time);
            if use_v0 {
                (entry.segment_duration as u32).encode(buf)?;
                (media_time as i32).encode(buf)?;
            } else {
                entry.segment_duration.encode(buf)?;
                media_time.encode(buf)?;
            }
            entry.media_rate.encode(buf)?;
        }

        Ok(if use_v0 {
            ElstVersion::V0.into()
        } else {
            ElstVersion::V1.into()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elst32() {
        let expected = Elst {
            entries: vec![ElstEntry {
                segment_duration: 634634,
                media_time: Some(0),
                media_rate: 1.into(),
            }],
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();
        assert_eq!(buf[8], 0, "values within 32 bits encode as version 0");

        let mut buf = buf.as_ref();
        let decoded = Elst::decode(&mut buf).unwrap();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn test_elst64() {
        let expected = Elst {
            entries: vec![ElstEntry {
                segment_duration: 5_000_000_000,
                media_time: Some(5_000_000_000),
                media_rate: 1.into(),
            }],
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();
        assert_eq!(buf[8], 1, "values beyond 32 bits force version 1");

        let mut buf = buf.as_ref();
        let decoded = Elst::decode(&mut buf).unwrap();
        assert_eq!(decoded, expected);
    }

    // media_time is a *signed* int(32) in version 0, so any value above i32::MAX (up to
    // i64::MAX) must fall back to version 1 -- encoding it as a 32-bit int would overflow
    // into a negative media_time. i32::MAX + 1 is the smallest such value.
    #[test]
    fn test_elst_media_time_above_i32_forces_v1() {
        let expected = Elst {
            entries: vec![ElstEntry {
                segment_duration: 0,
                media_time: Some(i32::MAX as i64 + 1),
                media_rate: 1.into(),
            }],
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();
        assert_eq!(buf[8], 1, "media_time above i32::MAX must use version 1");

        let decoded = Elst::decode(&mut buf.as_ref()).unwrap();
        assert_eq!(decoded, expected);
    }

    // Regression: the "empty edit" (media_time = -1) must round-trip as `None`.
    // Decoding the 32-bit form as unsigned (the old bug) turned -1 into +4294967295,
    // which shifted the track by 2^32 media ticks and left video/audio on disjoint
    // timelines -- a black screen in the browser's Media Source Extensions.
    #[test]
    fn test_elst_empty_edit_sentinel() {
        let expected = Elst {
            entries: vec![
                ElstEntry {
                    segment_duration: 23,
                    media_time: None,
                    media_rate: 1.into(),
                },
                ElstEntry {
                    segment_duration: 0,
                    media_time: Some(0),
                    media_rate: 1.into(),
                },
            ],
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();
        let decoded = Elst::decode(&mut buf.as_ref()).unwrap();
        assert_eq!(decoded, expected);
    }

    // Decode a real version-0 edit list (as written by ffmpeg / L-SMASH etc.) whose
    // media_time is the 32-bit -1 (0xFFFFFFFF), and confirm we recover `None` and
    // preserve it through a re-encode (the decode -> re-encode path a repackager takes).
    #[test]
    fn test_elst_v0_neg1_decode_reencode() {
        let raw: &[u8] = &[
            0x00, 0x00, 0x00, 0x1C, // box size = 28
            b'e', b'l', b's', b't', //
            0x00, 0x00, 0x00, 0x00, // version 0, flags 0
            0x00, 0x00, 0x00, 0x01, // entry_count = 1
            0x00, 0x00, 0x03, 0xE8, // segment_duration = 1000
            0xFF, 0xFF, 0xFF, 0xFF, // media_time = -1 (32-bit)
            0x00, 0x01, 0x00, 0x00, // media_rate = 1.0
        ];
        let decoded = Elst::decode(&mut &raw[..]).unwrap();
        assert_eq!(decoded.entries[0].media_time, None);

        // A version-0 input round-trips byte-for-byte: the empty edit stays a 32-bit
        // -1, not a re-widened (and previously corrupted) 64-bit value.
        let mut buf = Vec::new();
        decoded.encode(&mut buf).unwrap();
        assert_eq!(buf.as_slice(), raw);
    }

    // A media_time below -1 is out of spec, and files carry it anyway. ffmpeg
    // rejects it only under FF_COMPLIANCE_STRICT (`mov_read_elst`), so refusing
    // it here would fail the whole `moov` for a file ffmpeg opens. It is kept as
    // written -- an ordinary edit, NOT the -1 empty edit -- and round-trips
    // byte-for-byte. Covered for both on-wire widths (v0 int(32), v1 int(64)).
    #[test]
    fn test_elst_negative_media_time_kept_v0() {
        let raw: &[u8] = &[
            0x00, 0x00, 0x00, 0x1C, // box size = 28
            b'e', b'l', b's', b't', //
            0x00, 0x00, 0x00, 0x00, // version 0, flags 0
            0x00, 0x00, 0x00, 0x01, // entry_count = 1
            0x00, 0x00, 0x03, 0xE8, // segment_duration = 1000
            0xFF, 0xFF, 0xFF, 0xFE, // media_time = -2 (out of spec, 32-bit)
            0x00, 0x01, 0x00, 0x00, // media_rate = 1.0
        ];
        let decoded = Elst::decode(&mut &raw[..]).unwrap();
        assert_eq!(decoded.entries[0].media_time, Some(-2));

        let mut buf = Vec::new();
        decoded.encode(&mut buf).unwrap();
        assert_eq!(buf.as_slice(), raw);
    }

    #[test]
    fn test_elst_negative_media_time_kept_v1() {
        let raw: &[u8] = &[
            0x00, 0x00, 0x00, 0x24, // box size = 36
            b'e', b'l', b's', b't', //
            0x01, 0x00, 0x00, 0x00, // version 1, flags 0
            0x00, 0x00, 0x00, 0x01, // entry_count = 1
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
            0xE8, // segment_duration = 1000 (64-bit)
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFE, // media_time = -2 (out of spec, 64-bit)
            0x00, 0x01, 0x00, 0x00, // media_rate = 1.0
        ];
        let decoded = Elst::decode(&mut &raw[..]).unwrap();
        assert_eq!(decoded.entries[0].media_time, Some(-2));
        // A negative that fits int(32) re-encodes as the compact version 0.
        let mut buf = Vec::new();
        decoded.encode(&mut buf).unwrap();
        assert_eq!(buf[8], 0);
        assert_eq!(Elst::decode(&mut buf.as_ref()).unwrap(), decoded);
    }

    // A negative below int(32) forces version 1, like any other out-of-range value.
    #[test]
    fn test_elst_media_time_below_i32_forces_v1() {
        let expected = Elst {
            entries: vec![ElstEntry {
                segment_duration: 0,
                media_time: Some(i32::MIN as i64 - 1),
                media_rate: 1.into(),
            }],
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();
        assert_eq!(buf[8], 1, "media_time below i32::MIN must use version 1");
        assert_eq!(Elst::decode(&mut buf.as_ref()).unwrap(), expected);
    }
}
