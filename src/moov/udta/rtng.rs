use crate::*;

use super::strings::{decode_utf8_or_16, skip_trailing};

/// RatingBox (3GPP TS 26.244 clause 8): a rating of the movie by one entity
/// under one criterion, one per language.
///
/// The rating information is a null-terminated string in UTF-8 or, after a byte
/// order mark, UTF-16; it is kept as text and encodes as UTF-8. Bytes that do
/// not decode (not UTF-8, or a lone UTF-16 surrogate) read as U+FFFD, so a
/// decoded box does not keep its original bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rtng {
    pub entity: FourCC,
    pub criteria: FourCC,
    pub language: String,
    pub rating_info: String,
}

impl AtomExt for Rtng {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"rtng");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        let entity = FourCC::decode(buf)?;
        let criteria = FourCC::decode(buf)?;
        let language_code = u16::decode(buf)?;
        let language = language_string(language_code);
        let rating_info = decode_utf8_or_16(buf);
        skip_trailing(Self::KIND_EXT, buf);
        Ok(Rtng {
            entity,
            criteria,
            language,
            rating_info,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.entity.encode(buf)?;
        self.criteria.encode(buf)?;
        let language_code = language_code(&self.language);
        language_code.encode(buf)?;
        self.rating_info.as_str().encode(buf)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    // An MPAA "G" rating, its terminator followed by five NUL bytes of padding.
    const ENCODED_RTNG: &[u8] = &[
        0x00, 0x00, 0x00, 0x1d, 0x72, 0x74, 0x6e, 0x67, 0x00, 0x00, 0x00, 0x00, 0x4d, 0x50, 0x41,
        0x41, 0x00, 0x00, 0x00, 0x00, 0x15, 0xc7, 0x47, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // The same rating, as it is written: the string and its terminator.
    const ENCODED_RTNG_UNPADDED: &[u8] = &[
        0x00, 0x00, 0x00, 0x18, 0x72, 0x74, 0x6e, 0x67, 0x00, 0x00, 0x00, 0x00, 0x4d, 0x50, 0x41,
        0x41, 0x00, 0x00, 0x00, 0x00, 0x15, 0xc7, 0x47, 0x00,
    ];

    fn rating(rating_info: &str) -> Rtng {
        Rtng {
            entity: b"MPAA".into(),
            criteria: b"\0\0\0\0".into(),
            language: "eng".into(),
            rating_info: rating_info.into(),
        }
    }

    // The rating information ends at its terminator; what follows is padding.
    #[test]
    fn test_rtng_decode() {
        let buf: &mut std::io::Cursor<&&[u8]> = &mut std::io::Cursor::new(&ENCODED_RTNG);
        let rtng = Rtng::decode(buf).expect("failed to decode rtng");
        assert_eq!(rtng, rating("G"));
    }

    #[test]
    fn test_rtng_encode() {
        let mut buf = Vec::new();
        rating("G").encode(&mut buf).unwrap();
        assert_eq!(buf, ENCODED_RTNG_UNPADDED);
        assert_eq!(Rtng::decode(&mut buf.as_slice()).unwrap(), rating("G"));
    }

    // UTF-16 after a byte order mark reads as its text and is written back as
    // UTF-8; text that is neither reads with U+FFFD for what does not decode.
    #[test]
    fn test_rtng_text() {
        let mut utf16 = ENCODED_RTNG_UNPADDED[..22].to_vec();
        utf16.extend_from_slice(&[0xfe, 0xff, 0x00, b'G', 0x00, 0xe9, 0x00, 0x00]);
        utf16[3] = utf16.len() as u8;
        let rtng = Rtng::decode(&mut utf16.as_slice()).unwrap();
        assert_eq!(rtng, rating("G\u{e9}"));

        let mut encoded = Vec::new();
        rtng.encode(&mut encoded).unwrap();
        assert_eq!(Rtng::decode(&mut encoded.as_slice()).unwrap(), rtng);

        let mut latin1 = ENCODED_RTNG_UNPADDED[..22].to_vec();
        latin1.extend_from_slice(b"G\xe9\0");
        latin1[3] = latin1.len() as u8;
        assert_eq!(
            Rtng::decode(&mut latin1.as_slice()).unwrap(),
            rating("G\u{fffd}")
        );
    }
}
