use crate::*;

use super::strings::skip_trailing;

/// One chapter entry inside a [`Chpl`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChplEntry {
    /// Start time, in 100ns units, relative to the start of the presentation.
    pub start: u64,
    /// The title, stored in UTF-8 with a one-byte length. Bytes that are not
    /// UTF-8 read as U+FFFD, and the title is written back as UTF-8, so a
    /// decoded box does not keep its original bytes. A title longer than 255
    /// bytes is written cut at the last character that fits (FFmpeg's writer
    /// cuts it at 255 bytes too).
    pub title: String,
}

impl Decode for ChplEntry {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self> {
        let start = u64::decode(buf)?;

        let title_len = u8::decode(buf)? as usize;
        if title_len > buf.remaining() {
            return Err(Error::OutOfBounds);
        }
        // UTF-8, as Nero and FFmpeg write it. A title that is not reads with
        // U+FFFD for what does not decode, rather than costing the list
        // (libavformat keeps its bytes as they are).
        let title = String::from_utf8_lossy(buf.slice(title_len)).into_owned();
        buf.advance(title_len);

        Ok(Self { start, title })
    }
}

impl Encode for ChplEntry {
    fn encode<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.start.encode(buf)?;

        // A title read with U+FFFD can be longer than the 255 bytes it took on
        // disk, so the encoder bounds it rather than refusing it.
        let title = bounded_str(&self.title, u8::MAX as usize, "chpl title");
        (title.len() as u8).encode(buf)?;
        buf.append_slice(title.as_bytes());

        Ok(())
    }
}

/// Nero-style Chapter List Box (`chpl`).
///
/// Not part of the ISO/QuickTime spec, but a de facto standard written and read
/// by e.g. Nero, `ffmpeg`'s `mov` muxer/demuxer, and MP4Box, as an alternative
/// to the QuickTime `chap` track reference + text track convention.
///
/// Version 0 and version 1 only differ in an extra 4-byte reserved field
/// present after the FullBox header in version 1 (absent in version 0); it
/// carries no known meaning in either case, so it's not exposed here. This
/// always encodes as version 1, matching the common convention used by
/// `ffmpeg`'s `mov` muxer and MP4Box.
///
/// An entry the box cuts short ends the list, keeping the chapters before it
/// (an error under `strict`), and bytes after the last entry are passed over,
/// as libavformat reads the box.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Chpl {
    pub entries: Vec<ChplEntry>,
}

impl Atom for Chpl {
    const KIND: FourCC = FourCC::new(b"chpl");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let version = u8::decode(buf)?;
        <[u8; 3]>::decode(buf)?; // flags
        if version != 0 {
            <[u8; 4]>::decode(buf)?; // reserved, version 1 only
        }

        let count = u8::decode(buf)?;
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            match ChplEntry::decode(buf) {
                Ok(entry) => entries.push(entry),
                // An entry the box cuts short ends the list, and the chapters
                // before it are kept, as libavformat keeps them. Under
                // `strict` the box is malformed.
                Err(Error::OutOfBounds) if !cfg!(feature = "strict") => {
                    tracing::warn!(
                        kind = %Self::KIND,
                        count,
                        kept = entries.len(),
                        "chapter list cut short inside an entry"
                    );
                    buf.advance(buf.remaining());
                    break;
                }
                Err(err) => return Err(err),
            }
        }
        skip_trailing(Self::KIND, buf);

        Ok(Self { entries })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        1u8.encode(buf)?; // version
        [0u8; 3].encode(buf)?; // flags
        [0u8; 4].encode(buf)?; // reserved

        let count: u8 = self
            .entries
            .len()
            .try_into()
            .map_err(|_| Error::TooLarge(Self::KIND))?;
        count.encode(buf)?;
        self.entries.encode(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chpl_roundtrip() {
        let expected = Chpl {
            entries: vec![
                ChplEntry {
                    start: 0,
                    title: "Intro".into(),
                },
                ChplEntry {
                    start: 30_000_000,
                    title: "Chapter 2".into(),
                },
            ],
        };

        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let decoded = Chpl::decode(&mut buf.as_slice()).expect("failed to decode chpl");
        assert_eq!(decoded, expected);
    }

    // A title that is not UTF-8 reads with U+FFFD for what does not decode,
    // and the chapters after it still read.
    #[test]
    fn test_chpl_lenient_title() {
        let mut body = vec![1, 0, 0, 0, 0, 0, 0, 0, 2]; // version 1, 2 chapters
        body.extend_from_slice(&0u64.to_be_bytes());
        body.push(4);
        body.extend_from_slice(b"Caf\xe9");
        body.extend_from_slice(&30_000_000u64.to_be_bytes());
        body.push(5);
        body.extend_from_slice(b"Outro");
        let mut atom = (body.len() as u32 + 8).to_be_bytes().to_vec();
        atom.extend_from_slice(b"chpl");
        atom.extend_from_slice(&body);

        let decoded = Chpl::decode(&mut atom.as_slice()).unwrap();
        assert_eq!(
            decoded,
            Chpl {
                entries: vec![
                    ChplEntry {
                        start: 0,
                        title: "Caf\u{fffd}".into(),
                    },
                    ChplEntry {
                        start: 30_000_000,
                        title: "Outro".into(),
                    },
                ],
            }
        );
    }

    // A version 1 `chpl` of `count` entries, with `entries` as stored.
    fn atom(count: u8, entries: &[u8]) -> Vec<u8> {
        let body = [&[1, 0, 0, 0, 0, 0, 0, 0, count][..], entries].concat();
        let mut atom = (body.len() as u32 + 8).to_be_bytes().to_vec();
        atom.extend_from_slice(b"chpl");
        atom.extend_from_slice(&body);
        atom
    }

    // An entry as stored: `start ‖ title length ‖ title`.
    fn entry(start: u64, title: &[u8]) -> Vec<u8> {
        [&start.to_be_bytes()[..], &[title.len() as u8], title].concat()
    }

    // A title read with U+FFFD can outgrow the 255 bytes it took on disk; it
    // is written back cut at the last character that fits, not refused.
    #[test]
    fn test_chpl_lossy_title_bounded() {
        let title = [&[b'a'; 254][..], b"\xe9"].concat();
        let chpl = Chpl::decode(&mut atom(1, &entry(0, &title)).as_slice()).unwrap();
        let read = &chpl.entries[0].title;
        assert_eq!(read.len(), 257);
        assert!(read.ends_with('\u{fffd}'));

        let mut encoded = Vec::new();
        chpl.encode(&mut encoded).unwrap();
        assert_eq!(encoded, atom(1, &entry(0, &[b'a'; 254])));
        let decoded = Chpl::decode(&mut encoded.as_slice()).unwrap();
        assert_eq!(decoded.entries[0].title, "a".repeat(254));
    }

    // Bytes after the last entry are passed over and the list is kept, in the
    // box and in the `udta` that holds it.
    #[test]
    fn test_chpl_trailing_bytes() {
        let bytes = atom(1, &[entry(0, b"Intro"), vec![0, 0]].concat());
        let expected = Chpl {
            entries: vec![ChplEntry {
                start: 0,
                title: "Intro".into(),
            }],
        };
        assert_eq!(Chpl::decode(&mut bytes.as_slice()).unwrap(), expected);

        let mut udta = ((bytes.len() + 8) as u32).to_be_bytes().to_vec();
        udta.extend_from_slice(b"udta");
        udta.extend_from_slice(&bytes);
        let udta = Udta::decode(&mut udta.as_slice()).unwrap();
        assert_eq!(udta.chpl, Some(expected));
    }

    // An entry the box cuts short, in its start time or in its title, ends
    // the list and keeps the chapters before it.
    #[cfg(not(feature = "strict"))]
    #[test]
    fn test_chpl_cut_short() {
        let intro = entry(0, b"Intro");
        let cut_title = entry(30_000_000, b"Chapter 2");
        let cut_title = &cut_title[..cut_title.len() - 4];
        let expected = Chpl {
            entries: vec![ChplEntry {
                start: 0,
                title: "Intro".into(),
            }],
        };
        for cut in [&[0, 0, 0][..], cut_title] {
            let bytes = atom(3, &[&intro[..], cut].concat());
            assert_eq!(Chpl::decode(&mut bytes.as_slice()).unwrap(), expected);
        }
    }

    // Under `strict` an entry cut short makes the box malformed.
    #[cfg(feature = "strict")]
    #[test]
    fn test_chpl_cut_short() {
        let bytes = atom(2, &[entry(0, b"Intro"), vec![0, 0, 0]].concat());
        match Chpl::decode(&mut bytes.as_slice()) {
            Err(Error::OverDecode(kind)) => assert_eq!(kind, Chpl::KIND),
            other => panic!("expected OverDecode(chpl), got {other:?}"),
        }
    }

    #[test]
    fn test_chpl_empty() {
        let expected = Chpl { entries: vec![] };

        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let decoded = Chpl::decode(&mut buf.as_slice()).expect("failed to decode chpl");
        assert_eq!(decoded, expected);
    }

    #[test]
    fn test_chpl_version0_no_reserved_field() {
        // Per ffmpeg's mov.c (mov_read_chpl): version 0 omits the 4-byte
        // reserved field that version 1 has after the FullBox header.
        let expected = Chpl {
            entries: vec![
                ChplEntry {
                    start: 0,
                    title: "Intro".into(),
                },
                ChplEntry {
                    start: 30_000_000,
                    title: "Chapter 2".into(),
                },
            ],
        };

        let mut buf = Vec::new();
        0u8.encode(&mut buf).unwrap(); // version 0
        [0u8; 3].encode(&mut buf).unwrap(); // flags
        let count: u8 = expected.entries.len().try_into().unwrap();
        count.encode(&mut buf).unwrap();
        for entry in &expected.entries {
            entry.start.encode(&mut buf).unwrap();
            let title_bytes = entry.title.as_bytes();
            let title_len: u8 = title_bytes.len().try_into().unwrap();
            title_len.encode(&mut buf).unwrap();
            buf.extend_from_slice(title_bytes);
        }

        let decoded = Chpl::decode_body(&mut buf.as_slice()).expect("failed to decode chpl");
        assert_eq!(decoded, expected);
    }
}
