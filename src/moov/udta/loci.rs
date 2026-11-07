use crate::*;

use super::strings::{decode_utf8_or_16, skip_trailing};

/// LocationInformationBox (3GPP TS 26.244 §8.11): where a movie was shot, or
/// the place it shows. FFmpeg writes it for `-metadata location=…` in MP4.
///
/// The three coordinates are signed 16.16 fixed-point numbers, kept as stored:
/// degrees for longitude (east positive) and latitude (north positive), metres
/// above sea level for altitude. The three strings are stored in UTF-8 or, after
/// a byte order mark, UTF-16; they are kept as text and encode as UTF-8. Bytes
/// that do not decode (not UTF-8, or a lone UTF-16 surrogate) read as U+FFFD,
/// so a decoded box does not keep its original bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Loci {
    /// The language code as stored: an ISO 639-2/T code packed as in `mdhd`.
    pub language: u16,
    /// The place's name.
    pub name: String,
    /// 0: shooting location, 1: real location, 2: fictional location.
    pub role: u8,
    pub longitude: i32,
    pub latitude: i32,
    pub altitude: i32,
    /// The body the coordinates are on: `earth` for most.
    pub astronomical_body: String,
    pub additional_notes: String,
}

impl AtomExt for Loci {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"loci");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        let language = u16::decode(buf)?;
        let name = decode_utf8_or_16(buf);
        let role = u8::decode(buf)?;
        let longitude = i32::decode(buf)?;
        let latitude = i32::decode(buf)?;
        let altitude = i32::decode(buf)?;
        let astronomical_body = decode_utf8_or_16(buf);
        let additional_notes = decode_utf8_or_16(buf);
        skip_trailing(Self::KIND_EXT, buf);

        Ok(Loci {
            language,
            name,
            role,
            longitude,
            latitude,
            altitude,
            astronomical_body,
            additional_notes,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.language.encode(buf)?;
        self.name.as_str().encode(buf)?;
        self.role.encode(buf)?;
        self.longitude.encode(buf)?;
        self.latitude.encode(buf)?;
        self.altitude.encode(buf)?;
        self.astronomical_body.as_str().encode(buf)?;
        self.additional_notes.as_str().encode(buf)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // FFmpeg's `loci` for `-metadata location=-33.8568+151.2153+12.500/Sydney`
    // with `location-eng` beside it.
    const ENCODED_LOCI: &[u8] = &[
        0x00, 0x00, 0x00, 0x29, b'l', b'o', b'c', b'i', // size 41
        0x00, 0x00, 0x00, 0x00, // version + flags
        0x15, 0xc7, // `eng`
        b'S', b'y', b'd', b'n', b'e', b'y', 0x00, // name
        0x00, // role: shooting location
        0x00, 0x97, 0x37, 0x1d, // longitude 151.2153
        0xff, 0xde, 0x24, 0xa9, // latitude -33.8568
        0x00, 0x0c, 0x80, 0x00, // altitude 12.5
        b'e', b'a', b'r', b't', b'h', 0x00, // astronomical body
        0x00, // additional notes
    ];

    #[test]
    fn test_loci() {
        let loci = Loci::decode(&mut &ENCODED_LOCI[..]).unwrap();
        assert_eq!(
            loci,
            Loci {
                language: 0x15c7,
                name: "Sydney".into(),
                role: 0,
                longitude: 9910045,
                latitude: -2218839,
                altitude: 12 << 16 | 0x8000,
                astronomical_body: "earth".into(),
                additional_notes: "".into(),
            }
        );

        let mut encoded = Vec::new();
        loci.encode(&mut encoded).unwrap();
        assert_eq!(encoded, ENCODED_LOCI);
    }

    // A `loci` of `name`, `ENCODED_LOCI`'s coordinates, then `tail`.
    fn atom(name: &[u8], tail: &[u8]) -> Vec<u8> {
        let mut body = vec![0, 0, 0, 0, 0x15, 0xc7];
        body.extend_from_slice(name);
        body.push(0); // role
        body.extend_from_slice(&[
            0x00, 0x97, 0x37, 0x1d, 0xff, 0xde, 0x24, 0xa9, 0x00, 0x0c, 0x80, 0x00,
        ]);
        body.extend_from_slice(tail);
        let mut atom = (body.len() as u32 + 8).to_be_bytes().to_vec();
        atom.extend_from_slice(b"loci");
        atom.extend_from_slice(&body);
        atom
    }

    fn be(text: &str) -> Vec<u8> {
        let mut bytes = vec![0xfe, 0xff];
        text.encode_utf16()
            .for_each(|u| bytes.extend_from_slice(&u.to_be_bytes()));
        bytes.extend_from_slice(&[0, 0]);
        bytes
    }

    fn le(text: &str) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xfe];
        text.encode_utf16()
            .for_each(|u| bytes.extend_from_slice(&u.to_le_bytes()));
        bytes.extend_from_slice(&[0, 0]);
        bytes
    }

    // UTF-16 is legal in all three strings, big- or little-endian, and its
    // terminator is 16 bits wide, so the coordinates stay aligned.
    #[test]
    fn test_loci_utf16() {
        let tail = [be("earth"), le("n\u{f6}te")].concat();
        let loci = Loci::decode(&mut &atom(&be("Sydney"), &tail)[..]).unwrap();
        assert_eq!(loci.name, "Sydney");
        assert_eq!((loci.longitude, loci.latitude), (9910045, -2218839));
        assert_eq!(loci.astronomical_body, "earth");
        assert_eq!(loci.additional_notes, "n\u{f6}te");

        let loci = Loci::decode(&mut &atom(&le("Z\u{fc}rich"), b"earth\0\0")[..]).unwrap();
        assert_eq!(loci.name, "Z\u{fc}rich");
        assert_eq!((loci.longitude, loci.latitude), (9910045, -2218839));
    }

    // Text that is neither keeps the coordinates, and so does data after the
    // notes.
    #[test]
    fn test_loci_lenient() {
        let loci = Loci::decode(&mut &atom(b"M\xfcnchen\0", b"earth\0caf\xe9\0")[..]).unwrap();
        assert_eq!(loci.name, "M\u{fffd}nchen");
        assert_eq!(loci.additional_notes, "caf\u{fffd}");
        assert_eq!(loci.latitude, -2218839);

        let loci = Loci::decode(&mut &atom(b"Sydney\0", b"earth\0\0\xde\xad\xbe\xef")[..]).unwrap();
        assert_eq!(loci.name, "Sydney");
        assert_eq!(loci.astronomical_body, "earth");
    }

    // Both trailing strings may be missing; a string with no terminator ends
    // with the box, a UTF-16 one with an odd byte too.
    #[test]
    fn test_loci_short() {
        let loci = Loci::decode(&mut &atom(b"Sydney\0", b"")[..]).unwrap();
        assert_eq!(loci.astronomical_body, "");
        assert_eq!(loci.additional_notes, "");

        let loci = Loci::decode(&mut &atom(b"Sydney\0", b"earth")[..]).unwrap();
        assert_eq!(loci.astronomical_body, "earth");

        let loci = Loci::decode(&mut &atom(b"Sydney\0", b"\xfe\xff\x00e\x00")[..]).unwrap();
        assert_eq!(loci.astronomical_body, "e");
    }

    // Coordinates that are not there are still malformed.
    #[test]
    fn test_loci_truncated() {
        let whole = atom(b"Sydney\0", b"");
        let mut short = whole[..whole.len() - 2].to_vec();
        short[3] -= 2;
        assert!(Loci::decode(&mut &short[..]).is_err());
    }
}
