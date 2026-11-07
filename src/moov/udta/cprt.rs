use crate::*;

use super::strings::{decode_utf8_or_16, skip_trailing};

/// CopyrightBox (ISO/IEC 14496-12 §8.10.2): a copyright notice, one per
/// language.
///
/// The notice is stored in UTF-8 or, after a byte order mark, UTF-16; it is kept
/// as text and encodes as UTF-8. Bytes that do not decode (not UTF-8, or a lone
/// UTF-16 surrogate) read as U+FFFD, so a decoded box does not keep its
/// original bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Cprt {
    pub language: String,
    pub notice: String,
}

impl AtomExt for Cprt {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"cprt");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        let language_code = u16::decode(buf)?;
        let language = language_string(language_code);
        let notice = decode_utf8_or_16(buf);
        skip_trailing(Self::KIND_EXT, buf);

        Ok(Cprt { language, notice })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        let language_code = language_code(&self.language);
        language_code.encode(buf)?;
        self.notice.as_str().encode(buf)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A `cprt` of language `eng` and `notice` as stored.
    fn atom(notice: &[u8]) -> Vec<u8> {
        let mut body = vec![0, 0, 0, 0, 0x15, 0xc7];
        body.extend_from_slice(notice);
        let mut atom = (body.len() as u32 + 8).to_be_bytes().to_vec();
        atom.extend_from_slice(b"cprt");
        atom.extend_from_slice(&body);
        atom
    }

    // A UTF-16 notice after its byte order mark reads as its text, and is
    // written back as UTF-8, which reads as the same notice.
    #[test]
    fn test_cprt_utf16() {
        let mut notice = vec![0xfe, 0xff];
        "\u{a9} 2026 K\u{f6}ln"
            .encode_utf16()
            .for_each(|unit| notice.extend_from_slice(&unit.to_be_bytes()));
        notice.extend_from_slice(&[0, 0]);

        let cprt = Cprt::decode(&mut atom(&notice).as_slice()).unwrap();
        let expected = Cprt {
            language: "eng".into(),
            notice: "\u{a9} 2026 K\u{f6}ln".into(),
        };
        assert_eq!(cprt, expected);

        let mut encoded = Vec::new();
        cprt.encode(&mut encoded).unwrap();
        assert_eq!(encoded, atom("\u{a9} 2026 K\u{f6}ln\0".as_bytes()));
        assert_eq!(Cprt::decode(&mut encoded.as_slice()).unwrap(), expected);
    }

    // A notice that is not UTF-8 reads with U+FFFD for what does not decode,
    // and padding after its terminator is passed over: neither fails the box.
    #[test]
    fn test_cprt_lenient() {
        let cprt = Cprt::decode(&mut atom(b"(c) Caf\xe9\0\0\0").as_slice()).unwrap();
        assert_eq!(cprt.notice, "(c) Caf\u{fffd}");

        let mut encoded = Vec::new();
        cprt.encode(&mut encoded).unwrap();
        assert_eq!(Cprt::decode(&mut encoded.as_slice()).unwrap(), cprt);
    }
}
