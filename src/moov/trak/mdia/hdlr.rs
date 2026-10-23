use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Hdlr {
    pub handler: FourCC,
    pub name: String,
    /// QuickTime's component type — `mhlr` for a media handler (in `mdia`),
    /// `dhlr` for a data handler (in `minf`) — in the field ISO/IEC 14496-12
    /// reserves as `pre_defined`.
    ///
    /// `Some` writes QuickTime's layout (QTFF): this type, and the name as a
    /// Pascal string. `None` writes ISO's: zero, and a NUL-terminated name. A
    /// decoded box reports what it was written as, and its name either way.
    pub component_type: Option<FourCC>,
}

impl Hdlr {
    /// QuickTime's media handler component type.
    pub const MEDIA_HANDLER: FourCC = FourCC::new(b"mhlr");
    /// QuickTime's data handler component type.
    pub const DATA_HANDLER: FourCC = FourCC::new(b"dhlr");
}

impl Default for Hdlr {
    fn default() -> Self {
        Hdlr {
            handler: FourCC::new(b"none"),
            name: String::new(),
            component_type: None,
        }
    }
}

impl AtomExt for Hdlr {
    type Ext = ();
    const KIND_EXT: FourCC = FourCC::new(b"hdlr");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        let pre_defined = u32::decode(buf)?;
        let component_type = (pre_defined != 0).then(|| FourCC::from(pre_defined));
        let handler = FourCC::decode(buf)?;

        <[u8; 12]>::decode(buf)?; // reserved

        // A QuickTime name is a Pascal string: a length byte, that many bytes,
        // and at most NUL padding after. QuickTime writers also put C strings
        // here, so anything else reads as one. Either way the name ends at its
        // first NUL: some writers count their NUL padding into the Pascal
        // length.
        let rest = buf.slice(buf.remaining());
        let pascal = component_type.is_some()
            && rest.first().is_some_and(|&n| {
                let end = 1 + n as usize;
                end <= rest.len() && rest[end..].iter().all(|&b| b == 0)
            });
        let name = if pascal {
            let n = rest[0] as usize;
            let bytes = &rest[1..1 + n];
            let len = bytes.iter().position(|&b| b == 0).unwrap_or(n);
            let name = String::from_utf8_lossy(&bytes[..len]).into_owned();
            buf.advance(1 + n);
            name
        } else {
            String::decode(buf)?
        };

        // Skip any trailing padding
        if buf.has_remaining() {
            tracing::warn!("Skipped {} extra trailing bytes in hdlr", buf.remaining());
            buf.advance(buf.remaining());
        }

        Ok(Hdlr {
            handler,
            name,
            component_type,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        // pre-defined (ISO) / component type (QuickTime)
        self.component_type.map_or(0, u32::from).encode(buf)?;
        self.handler.encode(buf)?;

        // 12 bytes reserved (QuickTime: manufacturer, flags, flags mask)
        [0u8; 12].encode(buf)?;

        if self.component_type.is_some() {
            let name = self.name.as_bytes();
            let n = name.len().min(255);
            (n as u8).encode(buf)?;
            buf.append_slice(&name[..n]);
        } else {
            self.name.as_str().encode(buf)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hdlr() {
        let expected = Hdlr {
            handler: FourCC::new(b"vide"),
            name: String::from("VideoHandler"),
            component_type: None,
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let mut buf = buf.as_ref();
        let decoded = Hdlr::decode(&mut buf).unwrap();
        assert_eq!(decoded, expected);
    }

    /// QuickTime's media handler, byte for byte as FFmpeg's `movenc` writes it
    /// into a MOV.
    #[test]
    fn test_hdlr_quicktime() {
        let expected = Hdlr {
            handler: FourCC::new(b"vide"),
            name: String::from("VideoHandler"),
            component_type: Some(Hdlr::MEDIA_HANDLER),
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();
        let mut movenc = vec![0, 0, 0, 45];
        movenc.extend_from_slice(b"hdlr");
        movenc.extend_from_slice(&[0; 4]);
        movenc.extend_from_slice(b"mhlrvide");
        movenc.extend_from_slice(&[0; 12]);
        movenc.push(12);
        movenc.extend_from_slice(b"VideoHandler");
        assert_eq!(buf, movenc);
        assert_eq!(Hdlr::decode(&mut buf.as_slice()).unwrap(), expected);
        // A QuickTime writer's C-string name reads as one.
        let mut cstr = buf[..32].to_vec();
        cstr.extend_from_slice(b"VideoHandler\0");
        cstr[3] = cstr.len() as u8;
        assert_eq!(
            Hdlr::decode(&mut cstr.as_slice()).unwrap().name,
            "VideoHandler"
        );
    }

    /// A QuickTime media handler whose Pascal name length counts its NUL
    /// padding. The name ends at the first NUL.
    #[test]
    fn test_hdlr_quicktime_padded_name() {
        let mut padded = vec![0, 0, 0, 54];
        padded.extend_from_slice(b"hdlr");
        padded.extend_from_slice(&[0; 4]);
        padded.extend_from_slice(b"mhlrvide");
        padded.extend_from_slice(&[0; 12]);
        padded.push(21);
        padded.extend_from_slice(b"VideoHandler");
        padded.extend_from_slice(&[0; 9]);
        let decoded = Hdlr::decode(&mut padded.as_slice()).unwrap();
        assert_eq!(decoded.name, "VideoHandler");
        assert_eq!(decoded.component_type, Some(Hdlr::MEDIA_HANDLER));
    }

    #[test]
    fn test_hdlr_empty() {
        let expected = Hdlr {
            handler: FourCC::new(b"vide"),
            name: String::new(),
            component_type: None,
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let mut buf = buf.as_ref();
        let decoded = Hdlr::decode(&mut buf).unwrap();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn test_hdlr_with_trailing_bytes() {
        // Test that we can decode hdlr boxes with extra padding bytes after the name
        // Some encoders add extra null bytes or padding at the end of the box

        let mut buf = Vec::new();

        // hdlr box header
        buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // size (will be fixed later)
        buf.extend_from_slice(b"hdlr");

        // version and flags
        buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

        // pre-defined
        buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

        // handler type
        buf.extend_from_slice(b"vide");

        // reserved (12 bytes)
        buf.extend_from_slice(&[0x00; 12]);

        // name (null-terminated string)
        buf.extend_from_slice(b"VideoHandler\0");

        // Add extra trailing bytes that should be skipped
        buf.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]); // 4 extra bytes

        // Fix the size
        let size = buf.len() as u32;
        buf[0..4].copy_from_slice(&size.to_be_bytes());

        // Decode
        let mut cursor = std::io::Cursor::new(&buf);
        let decoded = Hdlr::decode(&mut cursor).expect("failed to decode hdlr with trailing bytes");

        // Verify the decoded data (trailing bytes should be ignored)
        assert_eq!(decoded.handler, FourCC::new(b"vide"));
        assert_eq!(decoded.name, "VideoHandler");

        // Verify we've consumed all the data
        assert_eq!(cursor.position(), buf.len() as u64);
    }

    #[test]
    fn test_hdlr_with_multiple_trailing_nulls() {
        // Test with multiple null bytes at the end (common in some encoders)

        let mut buf = Vec::new();

        // hdlr box header
        buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // size
        buf.extend_from_slice(b"hdlr");

        // version and flags
        buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

        // pre-defined
        buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

        // handler type
        buf.extend_from_slice(b"soun");

        // reserved (12 bytes)
        buf.extend_from_slice(&[0x00; 12]);

        // name
        buf.extend_from_slice(b"SoundHandler\0");

        // Add multiple null bytes at the end
        buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

        // Fix the size
        let size = buf.len() as u32;
        buf[0..4].copy_from_slice(&size.to_be_bytes());

        // Decode
        let mut cursor = std::io::Cursor::new(&buf);
        let decoded = Hdlr::decode(&mut cursor).expect("failed to decode hdlr with trailing nulls");

        // Verify
        assert_eq!(decoded.handler, FourCC::new(b"soun"));
        assert_eq!(decoded.name, "SoundHandler");
        assert_eq!(cursor.position(), buf.len() as u64);
    }

    #[test]
    fn test_hdlr_roundtrip_with_trailing_bytes() {
        // Test that our encoder doesn't add trailing bytes,
        // but our decoder can handle them

        let original = Hdlr {
            handler: FourCC::new(b"meta"),
            name: "MetaHandler".to_string(),
            component_type: None,
        };

        // Encode (should not have trailing bytes)
        let mut encoded = Vec::new();
        original.encode(&mut encoded).unwrap();

        // Decode the clean version
        let mut cursor = std::io::Cursor::new(&encoded);
        let decoded = Hdlr::decode(&mut cursor).expect("failed to decode clean hdlr");
        assert_eq!(decoded, original);

        // Now manually add trailing bytes to the encoded data
        let box_end = cursor.position() as usize;
        let mut encoded_with_trash = encoded[..box_end].to_vec();

        // Add some trash bytes
        encoded_with_trash.extend_from_slice(&[0xFF, 0xEE, 0xDD]);

        // Update the size in the header
        let new_size = encoded_with_trash.len() as u32;
        encoded_with_trash[0..4].copy_from_slice(&new_size.to_be_bytes());

        // Decode again - should still work and ignore trash
        let mut cursor2 = std::io::Cursor::new(&encoded_with_trash);
        let decoded2 = Hdlr::decode(&mut cursor2).expect("failed to decode hdlr with added trash");
        assert_eq!(decoded2, original);
        assert_eq!(cursor2.position(), encoded_with_trash.len() as u64);
    }
}
