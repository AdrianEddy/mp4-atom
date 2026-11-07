use crate::*;

/// A QuickTime user-data text item: a `udta` child whose type begins with `©`
/// (0xA9), such as `©xyz` (the location, as an ISO 6709 string), `©mak` and
/// `©mod` (the camera's make and model) or `©swr` (the software), or one of the
/// other types that hold text: `name` (a track's name), `manu` and `modl` (make
/// and model), `keyw`, `FIRM` (the firmware), `@PRM` and `@PRQ`. The QuickTime
/// File Format gives a `©` item's body as a list of strings, each one prefixed
/// by its length and language. Writers also put text there bare, with no
/// prefix (FFmpeg's `name`, a camera's `FIRM`): that, and whatever else
/// follows the last string, is kept as [`rest`].
///
/// [`rest`]: UdtaText::rest
///
/// The type is not fixed, so this is not an [`Atom`]: [`Udta`] decodes every
/// child [`UdtaText::is_text_kind`] names as one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UdtaText {
    pub kind: FourCC,
    /// One string per language, in file order.
    pub strings: Vec<UdtaString>,
    /// What follows the last string, verbatim: the whole body when it opens
    /// with no length prefix that fits it (bare text), else padding or a
    /// writer's own bytes.
    pub rest: Vec<u8>,
}

/// One string of a [`UdtaText`] item.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UdtaString {
    /// QuickTime's language code, as stored: a Macintosh language code below
    /// 0x400 (0 is English) or 0x7FFF (unspecified), whose text is in that
    /// language's Macintosh encoding; else an ISO 639-2/T code packed as in
    /// `mdhd`, whose text is UTF-8.
    pub language: u16,
    /// The text as stored, in the encoding its language code gives. No
    /// terminating NUL: the length prefix bounds it.
    pub text: Vec<u8>,
}

impl UdtaText {
    /// Whether a `udta` child of type `kind` is a text item: QuickTime
    /// reserves the types beginning with `©` (0xA9) for them, and `name`,
    /// `manu`, `modl`, `keyw`, `FIRM`, `@PRM` and `@PRQ` hold text too.
    pub fn is_text_kind(kind: FourCC) -> bool {
        kind.as_ref()[0] == 0xA9
            || matches!(
                kind.as_ref(),
                b"name" | b"manu" | b"modl" | b"keyw" | b"FIRM" | b"@PRM" | b"@PRQ"
            )
    }

    /// The strings while a length prefix fits what is left of the body, then
    /// the rest. No body is malformed: what is not a string is kept as it is.
    pub(crate) fn decode_body<B: Buf>(kind: FourCC, buf: &mut B) -> Self {
        let mut strings = Vec::new();
        while let Some(&[s0, s1, l0, l1]) = buf.slice(buf.remaining()).first_chunk::<4>() {
            let size = usize::from(u16::from_be_bytes([s0, s1]));
            if size > buf.remaining() - 4 {
                break;
            }
            buf.advance(4);
            strings.push(UdtaString {
                language: u16::from_be_bytes([l0, l1]),
                text: buf.slice(size).to_vec(),
            });
            buf.advance(size);
        }
        let rest = buf.slice(buf.remaining()).to_vec();
        buf.advance(rest.len());
        Self {
            kind,
            strings,
            rest,
        }
    }
}

impl Encode for UdtaText {
    fn encode<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        buf.encode_atom(self.kind, |buf| {
            for string in &self.strings {
                let size =
                    u16::try_from(string.text.len()).map_err(|_| Error::TooLarge(self.kind))?;
                size.encode(buf)?;
                string.language.encode(buf)?;
                string.text.as_slice().encode(buf)?;
            }
            self.rest.as_slice().encode(buf)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // FFmpeg's `©xyz` for `-metadata location=…`: one string, language `und`
    // (packed ISO 639-2/T 0x55C4).
    const ENCODED_XYZ: &[u8] = &[
        0x00, 0x00, 0x00, 0x1e, 0xa9, b'x', b'y', b'z', // size 30, ©xyz
        0x00, 0x12, 0x55, 0xc4, // 18 bytes, `und`
        b'+', b'2', b'7', b'.', b'9', b'8', b'8', b'1', b'+', b'0', b'8', b'6', b'.', b'9', b'2',
        b'5', b'0', b'/',
    ];

    #[test]
    fn test_udta_text_decode() {
        let mut buf = ENCODED_XYZ;
        let header = Header::decode(&mut buf).unwrap();
        assert!(UdtaText::is_text_kind(header.kind));
        let text = UdtaText::decode_body(header.kind, &mut buf);
        assert_eq!(
            text,
            UdtaText {
                kind: FourCC::new(b"\xa9xyz"),
                strings: vec![UdtaString {
                    language: 0x55c4,
                    text: b"+27.9881+086.9250/".to_vec(),
                }],
                rest: vec![],
            }
        );

        let mut encoded = Vec::new();
        text.encode(&mut encoded).unwrap();
        assert_eq!(encoded, ENCODED_XYZ);
    }

    // Several languages, a Macintosh language code (0, English; the text in
    // Mac Roman) and an empty string all round-trip.
    #[test]
    fn test_udta_text_strings() {
        let text = UdtaText {
            kind: FourCC::new(b"\xa9mod"),
            strings: vec![
                UdtaString {
                    language: 0,
                    text: b"Caf\x8e X1".to_vec(),
                },
                UdtaString {
                    language: 0x15c7, // `eng`
                    text: "Caf\u{e9} X1".as_bytes().to_vec(),
                },
                UdtaString {
                    language: 0x7fff,
                    text: Vec::new(),
                },
            ],
            rest: vec![],
        };
        let mut encoded = Vec::new();
        text.encode(&mut encoded).unwrap();

        let mut buf = encoded.as_slice();
        let header = Header::decode(&mut buf).unwrap();
        assert_eq!(UdtaText::decode_body(header.kind, &mut buf), text);
    }

    // What is not a string stays in `rest`, so nothing is lost and the item
    // re-encodes as it came: a bare body, a body under a prefix's four bytes,
    // a string stating more bytes than the item holds, and what follows a
    // string that fits (padding, or bytes no prefix fits).
    #[test]
    fn test_udta_text_bounds() {
        let ab = [0x00, 0x02, 0x55, 0xc4, b'a', b'b'];
        let long = [0x00, 0x09, 0x55, 0xc4, b'a', b'b'];
        for (body, strings, rest) in [
            (b"HD9.01.01.60.00".to_vec(), 0, b"HD9.01.01.60.00".to_vec()),
            (b"abc".to_vec(), 0, b"abc".to_vec()),
            (long.to_vec(), 0, long.to_vec()),
            ([&ab[..], &[0x00, 0x00][..]].concat(), 1, vec![0x00, 0x00]),
            ([&ab[..], &[0xff; 5][..]].concat(), 1, vec![0xff; 5]),
        ] {
            let text = UdtaText::decode_body(FourCC::new(b"\xa9mak"), &mut body.as_slice());
            assert_eq!(text.strings.len(), strings, "{body:?}");
            assert_eq!(text.rest, rest, "{body:?}");

            let mut encoded = Vec::new();
            text.encode(&mut encoded).unwrap();
            assert_eq!(&encoded[8..], body);
        }
    }

    #[test]
    fn test_udta_text_kind() {
        for kind in [
            b"\xa9swr", b"name", b"manu", b"modl", b"keyw", b"FIRM", b"@PRM", b"@PRQ",
        ] {
            assert!(UdtaText::is_text_kind(FourCC::new(kind)), "{kind:?}");
        }
        for kind in [b"XMP_", b"loci", b"meta", b"cprt"] {
            assert!(!UdtaText::is_text_kind(FourCC::new(kind)), "{kind:?}");
        }
    }
}
