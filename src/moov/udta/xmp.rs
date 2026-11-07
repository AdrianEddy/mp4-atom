use crate::*;

/// The XMP packet a QuickTime movie carries in `udta` (Adobe XMP Specification
/// Part 3, "QuickTime": the `XMP_` box), kept as stored: UTF-8 XML.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Xmp {
    pub data: Vec<u8>,
}

impl Atom for Xmp {
    const KIND: FourCC = FourCC::new(b"XMP_");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        Ok(Xmp {
            data: Vec::decode(buf)?,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.data.encode(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xmp() {
        let packet: &[u8] = br#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/"/><?xpacket end="w"?>"#;
        let mut encoded = (8 + packet.len() as u32).to_be_bytes().to_vec();
        encoded.extend_from_slice(b"XMP_");
        encoded.extend_from_slice(packet);

        let xmp = Xmp::decode(&mut encoded.as_slice()).unwrap();
        assert_eq!(xmp.data, packet);

        let mut reencoded = Vec::new();
        xmp.encode(&mut reencoded).unwrap();
        assert_eq!(reencoded, encoded);
    }
}
