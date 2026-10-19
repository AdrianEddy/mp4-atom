use crate::*;

ext! {
    name: Mdhd,
    versions: [0, 1],
    flags: {}
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mdhd {
    pub creation_time: u64,
    pub modification_time: u64,
    pub timescale: u32,
    pub duration: u64,
    pub language: String,
}

impl AtomExt for Mdhd {
    type Ext = MdhdExt;

    const KIND_EXT: FourCC = FourCC::new(b"mdhd");

    fn decode_body_ext<B: Buf>(buf: &mut B, ext: MdhdExt) -> Result<Self> {
        let (creation_time, modification_time, timescale, duration) = match ext.version {
            MdhdVersion::V1 => (
                u64::decode(buf)?,
                u64::decode(buf)?,
                u32::decode(buf)?,
                u64::decode(buf)?,
            ),
            MdhdVersion::V0 => (
                u32::decode(buf)? as u64,
                u32::decode(buf)? as u64,
                u32::decode(buf)?,
                u32::decode(buf)? as u64,
            ),
        };

        let language_code = u16::decode(buf)?;
        let language = language_string(language_code);

        u16::decode(buf)?; // pre-defined

        Ok(Mdhd {
            creation_time,
            modification_time,
            timescale,
            duration,
            language,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<MdhdExt> {
        let v1 = needs_version_1(&[self.creation_time, self.modification_time, self.duration]);
        encode_time(self.creation_time, v1, buf)?;
        encode_time(self.modification_time, v1, buf)?;
        self.timescale.encode(buf)?;
        encode_time(self.duration, v1, buf)?;

        let language_code = language_code(&self.language);
        (language_code).encode(buf)?;
        0u16.encode(buf)?; // pre-defined

        Ok(if v1 { MdhdVersion::V1 } else { MdhdVersion::V0 }.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header boxes take version 0 unless a value needs 64 bits — the
    /// form QuickTime readers expect.
    #[test]
    fn header_boxes_take_version_0_when_their_values_fit() {
        let version = |b: &[u8]| b[8];
        let mdhd = |duration| {
            let mut buf = Vec::new();
            Mdhd {
                timescale: 60_000,
                duration,
                language: "und".into(),
                ..Default::default()
            }
            .encode(&mut buf)
            .unwrap();
            buf
        };
        assert_eq!(
            (version(&mdhd(u64::from(u32::MAX))), mdhd(1).len()),
            (0, 32)
        );
        assert_eq!(
            (
                version(&mdhd(u64::from(u32::MAX) + 1)),
                mdhd(u64::from(u32::MAX) + 1).len()
            ),
            (1, 44)
        );
        let mut tkhd = Vec::new();
        Tkhd {
            track_id: 1,
            duration: 1001,
            ..Default::default()
        }
        .encode(&mut tkhd)
        .unwrap();
        assert_eq!((version(&tkhd), tkhd.len()), (0, 92));
        let mut mvhd = Vec::new();
        Mvhd {
            timescale: 1000,
            duration: 1001,
            ..Default::default()
        }
        .encode(&mut mvhd)
        .unwrap();
        assert_eq!((version(&mvhd), mvhd.len()), (0, 108));
        for (bytes, kind) in [(mdhd(5), "mdhd"), (tkhd, "tkhd"), (mvhd, "mvhd")] {
            let back = Any::decode(&mut bytes.as_slice()).unwrap();
            assert_eq!(
                back.kind(),
                FourCC::new(kind.as_bytes().try_into().unwrap())
            );
        }
    }

    #[test]
    fn test_mdhd32() {
        let expected = Mdhd {
            creation_time: 100,
            modification_time: 200,
            timescale: 48000,
            duration: 30439936,
            language: String::from("und"),
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let mut buf = buf.as_ref();
        let decoded = Mdhd::decode(&mut buf).unwrap();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn test_mdhd64() {
        let expected = Mdhd {
            creation_time: 100,
            modification_time: 200,
            timescale: 48000,
            duration: 30439936,
            language: String::from("eng"),
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let mut buf = buf.as_ref();
        let decoded = Mdhd::decode(&mut buf).unwrap();
        assert_eq!(decoded, expected);
    }
}
