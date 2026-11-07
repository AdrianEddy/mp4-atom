use crate::*;

use super::strings::{decode_utf8, skip_trailing};

/// KindBox (ISO/IEC 14496-12 §8.10.4): a role of the track, as a scheme URI
/// and a value in it.
///
/// Both are stored in UTF-8; they are kept as text and encode as UTF-8. Bytes
/// that are not UTF-8 read as U+FFFD, so a decoded box does not keep its
/// original bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Kind {
    pub scheme_uri: String,
    pub value: String,
}

impl AtomExt for Kind {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"kind");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        let scheme_uri = decode_utf8(buf);
        let value = decode_utf8(buf);
        skip_trailing(Self::KIND_EXT, buf);

        Ok(Kind { scheme_uri, value })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.scheme_uri.as_str().encode(buf)?;
        self.value.as_str().encode(buf)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Text that is not UTF-8 reads with U+FFFD for what does not decode, and
    // the value after it still reads.
    #[test]
    fn test_kind_lenient() {
        let mut body = vec![0, 0, 0, 0];
        body.extend_from_slice(b"urn:example:r\xf4le\0caf\xe9\0");
        let mut atom = (body.len() as u32 + 8).to_be_bytes().to_vec();
        atom.extend_from_slice(b"kind");
        atom.extend_from_slice(&body);

        let kind = Kind::decode(&mut atom.as_slice()).unwrap();
        assert_eq!(
            kind,
            Kind {
                scheme_uri: "urn:example:r\u{fffd}le".into(),
                value: "caf\u{fffd}".into(),
            }
        );

        let mut encoded = Vec::new();
        kind.encode(&mut encoded).unwrap();
        assert_eq!(Kind::decode(&mut encoded.as_slice()).unwrap(), kind);
    }
}
