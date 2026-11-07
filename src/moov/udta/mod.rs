mod chpl;
mod cprt;
mod kind;
mod loci;
mod rtng;
mod skip;
mod strings;
mod text;
mod xmp;

pub use chpl::*;
pub use cprt::*;
pub use kind::*;
pub use loci::*;
pub use rtng::*;
pub use skip::*;
pub use text::*;
pub use xmp::*;

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Udta {
    /// Zero or one, a Nero-style chapter list (not part of the ISO/QuickTime spec).
    pub chpl: Option<Chpl>,
    /// Zero or more, one per language (ISO/IEC 14496-12 §8.10.2).
    pub cprt: Vec<Cprt>,
    /// Zero or more, one per role/kind label (ISO/IEC 14496-12 §8.10.4).
    pub kind: Vec<Kind>,
    /// Zero or one (ISO/IEC 14496-12 §8.11.1).
    pub meta: Option<Meta>,
    /// Zero or more, one per language (3GPP TS 26.244 clause 8).
    pub rtng: Vec<Rtng>,
    /// QuickTime user-data text items (`©xyz`, `©mak`, `©mod`, `©swr`, `name`,
    /// `manu`, `modl`, …), in file order.
    pub text: Vec<UdtaText>,
    /// Zero or more, one per language (3GPP TS 26.244 clause 8).
    pub loci: Vec<Loci>,
    /// Zero or one (Adobe XMP Specification Part 3, "QuickTime").
    pub xmp: Option<Xmp>,
}

impl Atom for Udta {
    const KIND: FourCC = FourCC::new(b"udta");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut chpl = None;
        let mut cprt = Vec::new();
        let mut kind = Vec::new();
        let mut meta = None;
        let mut rtng = Vec::new();
        let mut text = Vec::new();
        let mut loci = Vec::new();
        let mut xmp = None;

        // `udta` is a free-form user-data container. QuickTime writes a
        // track-name `name` box here, whose fourcc collides with the iTunes
        // `ilst` Name item in the global atom table, so dispatch children by
        // header against the udta namespace only — the same shape as
        // `Ilst::decode_body` — rather than through `Any`. Unknown children go
        // to `decode_unknown` (a warning by default, an error under strict).
        while let Some(header) = Header::decode_maybe(buf)? {
            let Some(size) = header.size_within(buf.remaining()) else {
                // A child whose declared size exceeds what remains is truncated.
                return Err(Error::OutOfBounds);
            };
            // Multiplicity follows the specs: `cprt` and `kind` are "zero or
            // more" (ISO/IEC 14496-12 §8.10.2 — one copyright notice per
            // language — and §8.10.4 — one box per role value), and 3GPP
            // TS 26.244 clause 8 admits each asset box (`rtng` included) once
            // per language, so repeats accumulate. `meta` is "zero or one" per
            // container (ISO/IEC 14496-12 §8.11.1), a Nero chapter list is one
            // list, and there is one XMP packet per file, so a second `meta`,
            // `chpl` or `XMP_` is malformed: it is skipped and the first kept,
            // as a child that does not decode is (`skip_malformed`).
            // QuickTime keeps any number of text items, one per type and
            // language as a rule, in file order.
            match header.kind {
                Chpl::KIND if chpl.is_some() => {
                    skip_malformed(Chpl::KIND, size, buf, Error::DuplicateBox(Chpl::KIND))?
                }
                Chpl::KIND => chpl = decode_or_skip(&header, size, buf)?,
                Cprt::KIND => cprt.extend(decode_or_skip(&header, size, buf)?),
                Kind::KIND => kind.extend(decode_or_skip(&header, size, buf)?),
                Meta::KIND if meta.is_some() => {
                    skip_malformed(Meta::KIND, size, buf, Error::DuplicateBox(Meta::KIND))?
                }
                Meta::KIND => meta = decode_or_skip(&header, size, buf)?,
                Rtng::KIND => rtng.extend(decode_or_skip(&header, size, buf)?),
                Loci::KIND => loci.extend(decode_or_skip(&header, size, buf)?),
                Xmp::KIND if xmp.is_some() => {
                    skip_malformed(Xmp::KIND, size, buf, Error::DuplicateBox(Xmp::KIND))?
                }
                Xmp::KIND => xmp = Some(Xmp::decode_atom(&header, buf)?),
                kind if UdtaText::is_text_kind(kind) => {
                    text.push(UdtaText::decode_body(kind, &mut buf.slice(size)));
                    buf.advance(size);
                }
                // `free`/`skip` are padding boxes; drop them.
                Free::KIND | Skip::KIND => {
                    buf.advance(size);
                }
                unknown => {
                    let body = Vec::decode(&mut buf.slice(size))?;
                    buf.advance(size);
                    Self::decode_unknown(&Any::Unknown(unknown, body))?;
                }
            }
        }
        // Drop any sub-header padding remainder after the children.
        skip_trailing_padding(buf);

        Ok(Udta {
            chpl,
            cprt,
            kind,
            meta,
            rtng,
            text,
            loci,
            xmp,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.chpl.encode(buf)?;
        self.cprt.encode(buf)?;
        self.meta.encode(buf)?;
        self.kind.encode(buf)?;
        self.rtng.encode(buf)?;
        self.text.encode(buf)?;
        self.loci.encode(buf)?;
        self.xmp.encode(buf)?;
        Ok(())
    }
}

/// A child of `size` bytes typed as `T`, or `None` when it does not decode: it
/// is then skipped ([`skip_malformed`]).
fn decode_or_skip<T: Atom, B: Buf>(header: &Header, size: usize, buf: &mut B) -> Result<Option<T>> {
    match T::decode_atom(header, buf) {
        Ok(atom) => Ok(Some(atom)),
        Err(err) => skip_malformed(T::KIND, size, buf, err).map(|()| None),
    }
}

/// Skip a `chpl`, `cprt`, `kind`, `meta`, `rtng` or `loci` that does not
/// decode, or a second `chpl`, `meta` or `XMP_`, as an unknown child is
/// skipped: one malformed tag must not make the movie unreadable, and
/// libavformat never fails a movie over a user-data box. Under `strict` the
/// error stands.
fn skip_malformed<B: Buf>(kind: FourCC, size: usize, buf: &mut B, err: Error) -> Result<()> {
    if cfg!(feature = "strict") || cfg!(test) {
        tracing::error!(%kind, %err, "malformed udta child");
        return Err(err);
    }
    tracing::warn!(%kind, %err, "malformed udta child skipped");
    buf.advance(size);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_udta_empty() {
        let expected = Udta {
            chpl: None,
            cprt: vec![],
            meta: None,
            kind: vec![],
            rtng: vec![],
            text: vec![],
            loci: vec![],
            xmp: None,
        };

        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let mut buf = buf.as_ref();
        let output = Udta::decode(&mut buf).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_udta() {
        let expected = Udta {
            chpl: None,
            cprt: vec![Cprt {
                language: "und".into(),
                notice: "MIT or Apache".into(),
            }],
            meta: Some(Meta {
                hdlr: Hdlr {
                    handler: FourCC::new(b"fake"),
                    name: "".into(),
                    component_type: None,
                },
                items: vec![],
            }),
            kind: vec![Kind {
                scheme_uri: "http://www.w3.org/TR/html5/".into(),
                value: "".into(),
            }],
            rtng: vec![Rtng {
                entity: b"BBFC".into(),
                criteria: b"PG13".into(),
                language: "eng".into(),
                rating_info: "test info".into(),
            }],
            text: vec![
                UdtaText {
                    kind: FourCC::new(b"\xa9xyz"),
                    strings: vec![UdtaString {
                        language: 0x15c7,
                        text: b"+27.9881+086.9250/".to_vec(),
                    }],
                    rest: vec![],
                },
                UdtaText {
                    kind: FourCC::new(b"\xa9swr"),
                    strings: vec![UdtaString {
                        language: 0,
                        text: b"Example 1.0".to_vec(),
                    }],
                    rest: vec![],
                },
            ],
            loci: vec![Loci {
                language: 0x15c7,
                name: "Everest".into(),
                role: 0,
                longitude: 5696716,
                latitude: 1834227,
                altitude: 8848 << 16,
                astronomical_body: "earth".into(),
                additional_notes: "".into(),
            }],
            xmp: Some(Xmp {
                data: b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>".to_vec(),
            }),
        };

        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let mut buf = buf.as_ref();
        let output = Udta::decode(&mut buf).unwrap();
        assert_eq!(output, expected);
    }

    // From MPEG File Format Conformance, isobmff/02_dref_edts_img.mp4
    const ENCODED_UDTA_WITH_CPRT: &[u8] = &[
        0x00, 0x00, 0x00, 0x70, 0x75, 0x64, 0x74, 0x61, 0x00, 0x00, 0x00, 0x68, 0x63, 0x70, 0x72,
        0x74, 0x00, 0x00, 0x00, 0x00, 0x55, 0xc4, 0x45, 0x4e, 0x53, 0x54, 0x20, 0x49, 0x73, 0x6f,
        0x4d, 0x65, 0x64, 0x69, 0x61, 0x20, 0x43, 0x6f, 0x6e, 0x66, 0x6f, 0x72, 0x6d, 0x61, 0x6e,
        0x63, 0x65, 0x20, 0x46, 0x69, 0x6c, 0x65, 0x73, 0x20, 0x2d, 0x20, 0x45, 0x4e, 0x53, 0x54,
        0x20, 0x28, 0x63, 0x29, 0x20, 0x32, 0x30, 0x30, 0x36, 0x20, 0x2d, 0x20, 0x52, 0x69, 0x67,
        0x68, 0x74, 0x73, 0x20, 0x72, 0x65, 0x6c, 0x65, 0x61, 0x73, 0x65, 0x64, 0x20, 0x66, 0x6f,
        0x72, 0x20, 0x49, 0x53, 0x4f, 0x20, 0x43, 0x6f, 0x6e, 0x66, 0x6f, 0x72, 0x6d, 0x61, 0x6e,
        0x63, 0x65, 0x20, 0x75, 0x73, 0x65, 0x00,
    ];

    #[test]
    fn test_udta_cprt() {
        let mut buf = std::io::Cursor::new(ENCODED_UDTA_WITH_CPRT);

        let udta = Udta::decode(&mut buf).expect("failed to decode udta");

        assert_eq!(
            udta,
            Udta {
                cprt: vec![Cprt { language: "und".into(), notice: "ENST IsoMedia Conformance Files - ENST (c) 2006 - Rights released for ISO Conformance use".into() }],
                ..Default::default()
            }
        );

        let mut buf = Vec::new();
        udta.encode(&mut buf).unwrap();

        assert_eq!(buf, ENCODED_UDTA_WITH_CPRT);
    }

    // From MPEG File Format Conformance, nalu/hevc/hev1_clg1_header.mp4
    const ENCODED_UDTA_WITH_KIND: &[u8] = &[
        0x00, 0x00, 0x00, 0x31, 0x75, 0x64, 0x74, 0x61, 0x00, 0x00, 0x00, 0x29, 0x6b, 0x69, 0x6e,
        0x64, 0x00, 0x00, 0x00, 0x00, 0x75, 0x72, 0x6e, 0x3a, 0x6d, 0x70, 0x65, 0x67, 0x3a, 0x64,
        0x61, 0x73, 0x68, 0x3a, 0x72, 0x6f, 0x6c, 0x65, 0x3a, 0x32, 0x30, 0x31, 0x31, 0x00, 0x6d,
        0x61, 0x69, 0x6e, 0x00,
    ];

    #[test]
    fn test_udta_kind() {
        let mut buf = std::io::Cursor::new(ENCODED_UDTA_WITH_KIND);

        let udta = Udta::decode(&mut buf).expect("failed to decode udta");

        assert_eq!(
            udta,
            Udta {
                kind: vec![Kind {
                    scheme_uri: "urn:mpeg:dash:role:2011".into(),
                    value: "main".into()
                }],
                ..Default::default()
            }
        );

        let mut buf = Vec::new();
        udta.encode(&mut buf).unwrap();

        assert_eq!(buf, ENCODED_UDTA_WITH_KIND);
    }

    // A track-level QuickTime `udta` holding an empty (4-null-byte) track-name
    // `name` box. Its fourcc collides with the iTunes `ilst` Name item in the
    // global atom table.
    const ENCODED_UDTA_WITH_QT_NAME: &[u8] = &[
        0x00, 0x00, 0x00, 0x14, 0x75, 0x64, 0x74, 0x61, // udta, size 20
        0x00, 0x00, 0x00, 0x0c, 0x6e, 0x61, 0x6d, 0x65, // name, size 12
        0x00, 0x00, 0x00, 0x00, // body: empty QuickTime track-name string
    ];

    // A sub-header padding remainder after a `udta`'s children must be skipped
    // as padding while the real children still decode.
    #[test]
    fn test_udta_trailing_padding() {
        let mut buf = ENCODED_UDTA_WITH_CPRT.to_vec();
        buf.extend_from_slice(&[0, 0, 0, 0]);
        let size = (buf.len() as u32).to_be_bytes();
        buf[0..4].copy_from_slice(&size);

        let udta = Udta::decode(&mut buf.as_slice()).expect("trailing padding must be tolerated");
        assert_eq!(
            udta.cprt.len(),
            1,
            "the real child still decodes, the padding is dropped"
        );
    }

    #[test]
    fn test_udta_quicktime_name_not_misparsed() {
        // The udta-namespace dispatch must NOT route `name` to the global
        // `Any` table's ilst Name item (which would UnderDecode the 4-byte
        // body): it is a text item, here one empty string.
        let udta = Udta::decode(&mut std::io::Cursor::new(ENCODED_UDTA_WITH_QT_NAME)).unwrap();
        assert_eq!(
            udta.text,
            vec![UdtaText {
                kind: FourCC::new(b"name"),
                strings: vec![UdtaString {
                    language: 0,
                    text: vec![],
                }],
                rest: vec![],
            }]
        );

        let mut encoded = Vec::new();
        udta.encode(&mut encoded).unwrap();
        assert_eq!(encoded, ENCODED_UDTA_WITH_QT_NAME);
    }

    // FFmpeg's track `udta` for `-metadata:s:v title=…`: a `name` whose body is
    // the bare UTF-8 title, no length prefix.
    #[test]
    fn test_udta_bare_name() {
        let bytes = [
            &[0x00, 0x00, 0x00, 0x1c][..],
            b"udta",
            &[0x00, 0x00, 0x00, 0x14],
            b"name",
            b"Front camera",
        ]
        .concat();
        let udta = Udta::decode(&mut bytes.as_slice()).unwrap();
        assert_eq!(
            udta.text,
            vec![UdtaText {
                kind: FourCC::new(b"name"),
                strings: vec![],
                rest: b"Front camera".to_vec(),
            }]
        );

        let mut encoded = Vec::new();
        udta.encode(&mut encoded).unwrap();
        assert_eq!(encoded, bytes);
    }

    // Repeated `cprt`/`kind`/`rtng` children are spec-legal — ISO/IEC 14496-12
    // declares both CopyrightBox (§8.10.2) and KindBox (§8.10.4) as quantity
    // "zero or more" (one copyright per language, one kind per role value),
    // and 3GPP TS 26.244 clause 8 admits "zero or more sub-boxes of each kind,
    // zero or one for each language" of the asset boxes, `rtng` included.
    // Repeats must accumulate — not error, not overwrite each other.
    #[test]
    fn test_udta_repeated_children() {
        let expected = Udta {
            chpl: None,
            cprt: vec![
                Cprt {
                    language: "eng".into(),
                    notice: "All rights reserved".into(),
                },
                Cprt {
                    language: "fra".into(),
                    notice: "Tous droits réservés".into(),
                },
            ],
            kind: vec![
                Kind {
                    scheme_uri: "urn:mpeg:dash:role:2011".into(),
                    value: "main".into(),
                },
                Kind {
                    scheme_uri: "urn:mpeg:dash:role:2011".into(),
                    value: "caption".into(),
                },
            ],
            meta: None,
            rtng: vec![
                Rtng {
                    entity: b"BBFC".into(),
                    criteria: b"PG13".into(),
                    language: "eng".into(),
                    rating_info: "parental guidance".into(),
                },
                Rtng {
                    entity: b"BBFC".into(),
                    criteria: b"PG13".into(),
                    language: "fra".into(),
                    rating_info: "accord parental".into(),
                },
            ],
            text: vec![
                UdtaText {
                    kind: FourCC::new(b"\xa9cmt"),
                    strings: vec![UdtaString {
                        language: 0x15c7,
                        text: b"first".to_vec(),
                    }],
                    rest: vec![],
                },
                UdtaText {
                    kind: FourCC::new(b"\xa9cmt"),
                    strings: vec![UdtaString {
                        language: 0x15c7,
                        text: b"second".to_vec(),
                    }],
                    rest: vec![],
                },
            ],
            loci: vec![
                Loci {
                    language: 0x15c7,
                    name: "Warsaw".into(),
                    ..Default::default()
                },
                Loci {
                    language: 0x41ec,
                    name: "Warszawa".into(),
                    ..Default::default()
                },
            ],
            xmp: None,
        };

        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let mut buf = buf.as_ref();
        let output = Udta::decode(&mut buf).unwrap();
        assert_eq!(output, expected);
    }

    // `meta` is quantity "zero or one" per container (ISO/IEC 14496-12
    // §8.11.1) — unlike the repeatable children above, a second one is
    // malformed, never silently overwriting the first (outside `strict` and
    // tests it is skipped, the first kept).
    #[test]
    fn test_udta_duplicate_meta() {
        let meta = Meta {
            hdlr: Hdlr {
                handler: FourCC::new(b"fake"),
                name: "".into(),
                component_type: None,
            },
            items: vec![],
        };
        let mut body = Vec::new();
        meta.encode(&mut body).unwrap();
        meta.encode(&mut body).unwrap();

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
        bytes.extend_from_slice(b"udta");
        bytes.extend_from_slice(&body);

        match Udta::decode(&mut bytes.as_slice()) {
            Err(Error::DuplicateBox(kind)) => assert_eq!(kind, Meta::KIND),
            other => panic!("expected DuplicateBox(meta), got {other:?}"),
        }
    }

    // FFmpeg's `udta` for a MOV with `-metadata date=… make=… model=…
    // location=…`: four QuickTime text items, language `und`.
    const ENCODED_UDTA_QUICKTIME_TEXT: &[u8] = &[
        0x00, 0x00, 0x00, 0x68, 0x75, 0x64, 0x74, 0x61, 0x00, 0x00, 0x00, 0x16, 0xa9, 0x64, 0x61,
        0x79, 0x00, 0x0a, 0x55, 0xc4, 0x32, 0x30, 0x32, 0x36, 0x2d, 0x31, 0x31, 0x2d, 0x30, 0x37,
        0x00, 0x00, 0x00, 0x16, 0xa9, 0x6d, 0x61, 0x6b, 0x00, 0x0a, 0x55, 0xc4, 0x45, 0x78, 0x61,
        0x6d, 0x70, 0x6c, 0x65, 0x43, 0x61, 0x6d, 0x00, 0x00, 0x00, 0x16, 0xa9, 0x6d, 0x6f, 0x64,
        0x00, 0x0a, 0x55, 0xc4, 0x45, 0x78, 0x61, 0x6d, 0x70, 0x6c, 0x65, 0x20, 0x58, 0x31, 0x00,
        0x00, 0x00, 0x1e, 0xa9, 0x78, 0x79, 0x7a, 0x00, 0x12, 0x55, 0xc4, 0x2b, 0x32, 0x37, 0x2e,
        0x39, 0x38, 0x38, 0x31, 0x2b, 0x30, 0x38, 0x36, 0x2e, 0x39, 0x32, 0x35, 0x30, 0x2f,
    ];

    #[test]
    fn test_udta_quicktime_text() {
        let udta = Udta::decode(&mut &ENCODED_UDTA_QUICKTIME_TEXT[..]).unwrap();
        let item = |kind: &[u8; 4], text: &[u8]| UdtaText {
            kind: FourCC::new(kind),
            strings: vec![UdtaString {
                language: 0x55c4,
                text: text.to_vec(),
            }],
            rest: vec![],
        };
        assert_eq!(
            udta,
            Udta {
                text: vec![
                    item(b"\xa9day", b"2026-11-07"),
                    item(b"\xa9mak", b"ExampleCam"),
                    item(b"\xa9mod", b"Example X1"),
                    item(b"\xa9xyz", b"+27.9881+086.9250/"),
                ],
                ..Default::default()
            }
        );

        let mut encoded = Vec::new();
        udta.encode(&mut encoded).unwrap();
        assert_eq!(encoded, ENCODED_UDTA_QUICKTIME_TEXT);
    }

    // A text item whose string runs past it keeps its body as `rest`: the
    // item and the rest of the `udta` read, and re-encode as they came.
    #[test]
    fn test_udta_malformed_text() {
        let mut bytes = ENCODED_UDTA_QUICKTIME_TEXT.to_vec();
        bytes[16..18].copy_from_slice(&0x0bu16.to_be_bytes()); // `©day` states 11 bytes, holds 10
        let udta = Udta::decode(&mut bytes.as_slice()).unwrap();
        assert_eq!(udta.text.len(), 4);
        assert!(udta.text[0].strings.is_empty());
        assert_eq!(udta.text[0].rest, &bytes[16..30]);

        let mut encoded = Vec::new();
        udta.encode(&mut encoded).unwrap();
        assert_eq!(encoded, bytes);
    }

    // One XMP packet per file: a second `XMP_` is malformed (outside `strict`
    // and tests it is skipped, the first kept).
    #[test]
    fn test_udta_duplicate_xmp() {
        let xmp = Xmp {
            data: b"<x:xmpmeta/>".to_vec(),
        };
        let mut body = Vec::new();
        xmp.encode(&mut body).unwrap();
        xmp.encode(&mut body).unwrap();

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
        bytes.extend_from_slice(b"udta");
        bytes.extend_from_slice(&body);

        match Udta::decode(&mut bytes.as_slice()) {
            Err(Error::DuplicateBox(kind)) => assert_eq!(kind, Xmp::KIND),
            other => panic!("expected DuplicateBox(XMP_), got {other:?}"),
        }
    }

    // A Nero chapter list is one list: a second `chpl` is malformed, never
    // silently replacing the first (outside `strict` and tests it is skipped,
    // the first kept).
    #[test]
    fn test_udta_duplicate_chpl() {
        let chpl = |title: &str| Chpl {
            entries: vec![ChplEntry {
                start: 0,
                title: title.into(),
            }],
        };
        let mut body = Vec::new();
        chpl("First").encode(&mut body).unwrap();
        chpl("Second").encode(&mut body).unwrap();

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
        bytes.extend_from_slice(b"udta");
        bytes.extend_from_slice(&body);

        match Udta::decode(&mut bytes.as_slice()) {
            Err(Error::DuplicateBox(kind)) => assert_eq!(kind, Chpl::KIND),
            other => panic!("expected DuplicateBox(chpl), got {other:?}"),
        }
    }

    // `size ‖ kind ‖ body`.
    fn child(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut bytes = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(body);
        bytes
    }

    // A `udta` whose `cprt` notice is UTF-16 (ISO/IEC 14496-12 §8.10.2 allows
    // it after a byte order mark) and whose `kind`, `rtng` and `chpl` hold text
    // that is not UTF-8 reads whole, each with its text, and so does the text
    // item after them. Written back, every string is UTF-8, and it reads as the
    // same `udta`.
    #[test]
    fn test_udta_text_encodings() {
        let notice = [
            &[0, 0, 0, 0, 0x15, 0xc7, 0xfe, 0xff][..],
            &[0x00, 0xa9, 0x00, b' ', 0x00, b'K', 0x00, 0xf6, 0x00, 0x00],
        ]
        .concat();
        let kind = [&[0, 0, 0, 0][..], b"urn:example\0caf\xe9\0"].concat();
        let rtng = [&[0, 0, 0, 0][..], b"MPAA\0\0\0\0\x15\xc7G\xe9\0"].concat();
        // Version 0, one chapter at 0, a 4-byte title.
        let chpl = [&[0, 0, 0, 0, 1][..], &[0; 8], &[4], b"Caf\xe9"].concat();
        let title = [&[0, 5, 0x55, 0xc4][..], b"Title"].concat();
        let body = [
            child(b"cprt", &notice),
            child(b"kind", &kind),
            child(b"rtng", &rtng),
            child(b"chpl", &chpl),
            child(b"\xa9nam", &title),
        ]
        .concat();
        let bytes = child(b"udta", &body);

        let udta = Udta::decode(&mut bytes.as_slice()).unwrap();
        let expected = Udta {
            chpl: Some(Chpl {
                entries: vec![ChplEntry {
                    start: 0,
                    title: "Caf\u{fffd}".into(),
                }],
            }),
            cprt: vec![Cprt {
                language: "eng".into(),
                notice: "\u{a9} K\u{f6}".into(),
            }],
            kind: vec![Kind {
                scheme_uri: "urn:example".into(),
                value: "caf\u{fffd}".into(),
            }],
            rtng: vec![Rtng {
                entity: b"MPAA".into(),
                criteria: b"\0\0\0\0".into(),
                language: "eng".into(),
                rating_info: "G\u{fffd}".into(),
            }],
            text: vec![UdtaText {
                kind: FourCC::new(b"\xa9nam"),
                strings: vec![UdtaString {
                    language: 0x55c4,
                    text: b"Title".to_vec(),
                }],
                rest: vec![],
            }],
            ..Default::default()
        };
        assert_eq!(udta, expected);

        let mut encoded = Vec::new();
        udta.encode(&mut encoded).unwrap();
        assert_eq!(Udta::decode(&mut encoded.as_slice()).unwrap(), expected);
    }

    // A `cprt`, `kind`, `rtng`, `chpl` or `meta` too short for its fields is
    // malformed (outside `strict` and tests it is skipped, and the children
    // after it read).
    #[test]
    fn test_udta_truncated_children() {
        let cases: [(&[u8; 4], &[u8]); 5] = [
            (b"cprt", &[0, 0, 0, 0, 0x15]),
            (b"kind", &[0, 0]),
            (b"rtng", &[0, 0, 0, 0, b'M', b'P']),
            // Cut before its count: one cut inside an entry keeps the
            // entries before it (`chpl::tests::test_chpl_cut_short`).
            (b"chpl", &[1, 0, 0, 0, 0, 0, 0, 0]),
            (b"meta", &[0, 0, 0, 0]),
        ];
        for (kind, body) in cases {
            let bytes = child(b"udta", &child(kind, body));
            match Udta::decode(&mut bytes.as_slice()) {
                Err(Error::OverDecode(atom)) => assert_eq!(atom, FourCC::new(kind)),
                other => panic!("expected OverDecode({kind:?}), got {other:?}"),
            }
        }
    }
}
