//! The strings ISO/IEC 14496-12 and 3GPP TS 26.244 store in user-data boxes.
//!
//! Each is read as text, never refused: bytes that do not decode read as
//! U+FFFD. libavformat, which reads these boxes as raw bytes, never fails a
//! movie over the encoding of its metadata, and one mislabelled string must not
//! cost the fields after it either.

use crate::*;

/// ISO/IEC 14496-12 `utf8string` (`kind`'s scheme URI and value): UTF-8 up to
/// a NUL, or to the end of the box.
pub(super) fn decode_utf8<B: Buf>(buf: &mut B) -> String {
    let bytes = buf.slice(buf.remaining());
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let text = String::from_utf8_lossy(&bytes[..len]).into_owned();
    let read = (len + 1).min(bytes.len());
    buf.advance(read);
    text
}

/// ISO/IEC 14496-12 `utf8or16string` (`cprt`'s notice, §8.10.2) and the
/// strings of 3GPP TS 26.244 clause 8 (`rtng`, `loci`): UTF-8 up to a NUL, or
/// UTF-16 after a byte order mark (either byte order) up to a 16-bit NUL, or to
/// the end of the box either way. The mark is not part of the text.
pub(super) fn decode_utf8_or_16<B: Buf>(buf: &mut B) -> String {
    let bytes = buf.slice(buf.remaining());
    let (text, read) = match bytes {
        [0xFE, 0xFF, rest @ ..] => utf16(rest, String::from_utf16be_lossy),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, String::from_utf16le_lossy),
        _ => return decode_utf8(buf),
    };
    let read = read.min(bytes.len());
    buf.advance(read);
    text
}

/// The text of a UTF-16 string after its byte order mark, decoded straight
/// from its bytes up to its terminator, and the string's size with mark and
/// terminator. An odd byte at the end of the box is not a code unit.
fn utf16(bytes: &[u8], decode: fn(&[u8]) -> String) -> (String, usize) {
    let units = bytes.as_chunks::<2>().0;
    let len = units
        .iter()
        .position(|&unit| unit == [0, 0])
        .unwrap_or(units.len());
    (decode(&bytes[..2 * len]), 2 * len + 4)
}

/// Pass over what a box holds after its last field (padding after a string,
/// as some writers leave), with a warning.
pub(super) fn skip_trailing<B: Buf>(kind: FourCC, buf: &mut B) {
    if buf.has_remaining() {
        tracing::warn!(%kind, "Skipped {} extra trailing bytes", buf.remaining());
        buf.advance(buf.remaining());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A UTF-8 string ends at its NUL, or with the box; what is not UTF-8 reads
    // as U+FFFD, and the string after it still reads.
    #[test]
    fn test_decode_utf8() {
        let mut buf = &b"Caf\xe9\0next"[..];
        assert_eq!(decode_utf8(&mut buf), "Caf\u{fffd}");
        assert_eq!(decode_utf8(&mut buf), "next");
        assert!(buf.is_empty());
        assert_eq!(decode_utf8(&mut buf), "");

        // A byte order mark is not one here: `utf8string` is UTF-8 only.
        let mut buf = &[0xfe, 0xff, 0, b'A', 0, 0][..];
        assert_eq!(decode_utf8(&mut buf), "\u{fffd}\u{fffd}");
    }

    // UTF-16 after a byte order mark, in either byte order, ends at a 16-bit
    // NUL; UTF-8 otherwise. A lone surrogate reads as U+FFFD.
    #[test]
    fn test_decode_utf8_or_16() {
        let mut buf = &[0xfe, 0xff, 0, b'A', 0xd8, 0x00, 0, b'B', 0, 0, b'x', 0][..];
        assert_eq!(decode_utf8_or_16(&mut buf), "A\u{fffd}B");
        assert_eq!(decode_utf8_or_16(&mut buf), "x");

        let mut buf = &[0xff, 0xfe, 0xa9, 0x00, 0, 0][..];
        assert_eq!(decode_utf8_or_16(&mut buf), "\u{a9}");
        assert!(buf.is_empty());

        // No terminator, and an odd byte: the string ends with the box.
        let mut buf = &[0xfe, 0xff, 0, b'A', 0][..];
        assert_eq!(decode_utf8_or_16(&mut buf), "A");
        assert!(buf.is_empty());

        let mut buf = &b"Caf\xe9"[..];
        assert_eq!(decode_utf8_or_16(&mut buf), "Caf\u{fffd}");
    }
}
