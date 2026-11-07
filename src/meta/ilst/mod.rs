mod covr;
mod cprt;
mod data;
mod desc;
mod name;
mod tool;
mod year;

pub use covr::*;
pub use cprt::*;
pub use data::*;
pub use desc::*;
pub use name::*;
pub use tool::*;
pub use year::*;

use crate::*;

/// Below this value, a child's raw fourcc looks like a big-endian 1-based
/// index (top byte zero) rather than 4 ASCII/Latin1 tag characters — the
/// scheme Apple's `mdta`-keyed metadata (see [`Keys`]) uses for `ilst` items.
const MDTA_INDEX_LIMIT: u32 = 0x0100_0000;

#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Ilst {
    pub name: Option<Name>,
    pub year: Option<Year>, // Called day in the spec
    pub covr: Option<Covr>,
    pub desc: Option<Desc>,
    pub ctoo: Option<Tool>,      // 4CC: "©too"
    pub cprt: Option<Copyright>, // iTunes item, NOT the ISO CopyrightBox

    /// Apple `mdta`-keyed items, keyed by their 1-based index into the
    /// sibling [`Keys`] box's entries, each with its values in order.
    pub mdta: Vec<(u32, Vec<IlstData>)>,

    /// The `data`-wrapped items this crate does not type (`©nam`, `©ART`,
    /// `©alb`, `©cmt`, `aART`, `trkn`, …), keyed by their four-character code,
    /// in file order, each with its values in order.
    ///
    /// A typed field holds the first item of its kind that types, which need
    /// not be the first of its kind in the file. Any other item of a typed
    /// kind is here only when it wraps `data` atoms: a `name` or `day `
    /// wrapping one, a `desc`, `©too` or `cprt` that is not one UTF-8 value,
    /// and a repeat of a kind whose field is already filled. A repeat that
    /// wraps none (a bare `name` or `day ` string, a raw `covr`) is treated as
    /// an unknown item, as a `----` freeform item is: dropped with a warning,
    /// or an error under `strict`. A consumer that needs every value of a key
    /// reads its typed field and these items both.
    pub items: Vec<(FourCC, Vec<IlstData>)>,
}

impl Atom for Ilst {
    const KIND: FourCC = FourCC::new(b"ilst");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut name = None;
        let mut year = None;
        let mut covr = None;
        let mut desc = None;
        let mut ctoo = None;
        let mut cprt = None;
        let mut mdta = vec![];
        let mut items = vec![];

        // `ilst` children live in the iTunes metadata namespace, which reuses
        // fourccs of unrelated ISO atoms — an ilst `cprt` item wraps a `data`
        // atom while the ISO CopyrightBox is a FullBox with a language code.
        // Dispatch by header against the ilst namespace only, never through
        // the global `Any` table.
        while let Some(header) = Header::decode_maybe(buf)? {
            let Some(size) = header.size_within(buf.remaining()) else {
                // Truncated child: stop and let the caller's remaining-bytes
                // check report it, matching `Any::decode_maybe`.
                break;
            };
            let body = buf.slice(size);
            let kind = header.kind;

            // A typed field holds the first item of its kind that types. Every
            // other item that wraps `data` atoms is kept by its code (or its
            // index) with its values, as libavformat reads any such item, so
            // no one item fails the list. One that wraps none (a `----`
            // freeform item) is unknown.
            let typed = match kind {
                Name::KIND => type_once(&mut name, body),
                Year::KIND => type_once(&mut year, body),
                Covr::KIND => type_once(&mut covr, body),
                Desc::KIND => type_once(&mut desc, body),
                Tool::KIND => type_once(&mut ctoo, body),
                Copyright::KIND => type_once(&mut cprt, body),
                _ => false,
            };
            if !typed {
                let index = u32::from(kind);
                match IlstData::decode_all(&mut &body[..]) {
                    Ok(values) if index < MDTA_INDEX_LIMIT => mdta.push((index, values)),
                    Ok(values) => items.push((kind, values)),
                    Err(_) => Self::decode_unknown(&Any::Unknown(kind, body.to_vec()))?,
                }
            }

            buf.advance(size);
        }

        Ok(Ilst {
            name,
            year,
            covr,
            desc,
            ctoo,
            cprt,
            mdta,
            items,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.name.encode(buf)?;
        self.year.encode(buf)?;
        self.covr.encode(buf)?;
        self.desc.encode(buf)?;
        self.ctoo.encode(buf)?;
        self.cprt.encode(buf)?;

        for (index, values) in &self.mdta {
            if *index == 0 || *index >= MDTA_INDEX_LIMIT {
                // A top-byte-nonzero index would encode as a FourCC that
                // looks like (or collides with) a real ASCII/Latin1 tag,
                // silently changing meaning on the next decode.
                return Err(Error::Unsupported("mdta index out of range"));
            }

            buf.encode_atom(*index, |buf| encode_values(values, buf))?;
        }

        for (kind, values) in &self.items {
            if u32::from(*kind) < MDTA_INDEX_LIMIT {
                // It would decode as an `mdta` index.
                return Err(Error::Unsupported("ilst item code out of range"));
            }

            buf.encode_atom(*kind, |buf| encode_values(values, buf))?;
        }

        Ok(())
    }
}

/// Type an item into `slot` when the slot is empty and the item's body is one
/// whole `T`. Whether it did.
fn type_once<T: Atom>(slot: &mut Option<T>, mut body: &[u8]) -> bool {
    if slot.is_some() {
        return false;
    }
    match T::decode_body(&mut body) {
        Ok(atom) if body.is_empty() => {
            *slot = Some(atom);
            true
        }
        _ => false,
    }
}

/// An item's values, one `data` atom each, inside the one item.
fn encode_values<B: BufMut>(values: &[IlstData], buf: &mut B) -> Result<()> {
    values.iter().try_for_each(|data| data.encode(buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ilst() {
        let expected = Ilst {
            year: Year("src_year".to_string()).into(),
            ..Default::default()
        };

        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let mut buf = buf.as_ref();
        let decoded = Ilst::decode(&mut buf).unwrap();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn test_ilst_empty() {
        let expected = Ilst::default();
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();

        let mut buf = buf.as_ref();
        let decoded = Ilst::decode(&mut buf).unwrap();
        assert_eq!(decoded, expected);
    }

    // Build `[size:u32][fourcc][body]`.
    fn atom_box(fourcc: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(8 + body.len());
        v.extend_from_slice(&((8 + body.len()) as u32).to_be_bytes());
        v.extend_from_slice(fourcc);
        v.extend_from_slice(body);
        v
    }

    // An iTunes `cprt` item (as written by FFmpeg for `-metadata copyright`)
    // wraps a `data` atom. Pre-fix, Ilst dispatched it through the global
    // `Any` table where the fourcc collides with the ISO CopyrightBox, so the
    // whole decode failed with UnderDecode(cprt).
    #[test]
    fn test_ilst_itunes_cprt() {
        let text = b"(c) 2026 Example";
        let mut data_body = Vec::new();
        data_body.extend_from_slice(&1u32.to_be_bytes()); // type indicator: UTF-8
        data_body.extend_from_slice(&0u32.to_be_bytes()); // country + language
        data_body.extend_from_slice(text);
        let cprt_item = atom_box(b"cprt", &atom_box(b"data", &data_body));
        let encoded = atom_box(b"ilst", &cprt_item);

        let decoded = Ilst::decode(&mut encoded.as_slice()).unwrap();
        assert_eq!(
            decoded,
            Ilst {
                cprt: Some(Copyright {
                    country_indicator: 0,
                    language_indicator: 0,
                    text: "(c) 2026 Example".into(),
                }),
                ..Default::default()
            }
        );

        // The encoder writes the same long-style `data` layout back.
        let mut reencoded = Vec::new();
        decoded.encode(&mut reencoded).unwrap();
        assert_eq!(reencoded, encoded);
    }

    // An item this crate does not type is kept by its code when it wraps a
    // `data` atom (FFmpeg's `©nam`, `©ART`, `©cmt` for `-metadata title=…`,
    // `artist=…`, `comment=…`), in file order.
    #[test]
    fn test_ilst_untyped_items() {
        let data_atom = |text: &[u8]| {
            let mut data_body = Vec::new();
            data_body.extend_from_slice(&1u32.to_be_bytes()); // type indicator: UTF-8
            data_body.extend_from_slice(&0u32.to_be_bytes()); // country + language
            data_body.extend_from_slice(text);
            atom_box(b"data", &data_body)
        };
        let mut body = Vec::new();
        body.extend_from_slice(&atom_box(b"\xa9nam", &data_atom(b"Harbour")));
        body.extend_from_slice(&atom_box(b"\xa9ART", &data_atom(b"Some Artist")));
        let encoded = atom_box(b"ilst", &body);

        let decoded = Ilst::decode(&mut encoded.as_slice()).unwrap();
        let text = |s: &str| IlstData {
            country_indicator: 0,
            language_indicator: 0,
            value: IlstDataValue::Utf8(s.into()),
        };
        assert_eq!(
            decoded,
            Ilst {
                items: vec![
                    (FourCC::new(b"\xa9nam"), vec![text("Harbour")]),
                    (FourCC::new(b"\xa9ART"), vec![text("Some Artist")]),
                ],
                ..Default::default()
            }
        );

        let mut reencoded = Vec::new();
        decoded.encode(&mut reencoded).unwrap();
        assert_eq!(reencoded, encoded);
    }

    // A multi-valued tag is one item holding several `data` atoms (as `mutagen`
    // writes it): each atom is a value, and the item re-encodes as it came. A
    // `desc`, `©too` or `cprt` item that is not one UTF-8 value is kept with
    // the untyped ones.
    #[test]
    fn test_ilst_multi_value_items() {
        let data_atom = |type_indicator: u32, value: &[u8]| {
            let mut data_body = type_indicator.to_be_bytes().to_vec();
            data_body.extend_from_slice(&0u32.to_be_bytes()); // country + language
            data_body.extend_from_slice(value);
            atom_box(b"data", &data_body)
        };
        // The second `desc` atom is 136 (0x88) bytes long, which is not UTF-8:
        // read as the first value's text, it failed the whole `ilst`.
        let long = [b'B'; 120];
        let encoded = atom_box(
            b"ilst",
            &[
                atom_box(
                    &1u32.to_be_bytes(),
                    &[data_atom(1, b"A"), data_atom(1, b"B")].concat(),
                ),
                atom_box(
                    b"\xa9ART",
                    &[data_atom(1, b"Alice"), data_atom(1, b"Bob")].concat(),
                ),
                atom_box(
                    b"\xa9cmt",
                    &[data_atom(21, &[5]), data_atom(21, &[6])].concat(),
                ),
                atom_box(
                    b"desc",
                    &[data_atom(1, b"First"), data_atom(1, &long)].concat(),
                ),
                atom_box(b"cprt", &data_atom(2, &[0, b'x'])),
            ]
            .concat(),
        );

        let decoded = Ilst::decode(&mut encoded.as_slice()).unwrap();
        let value = |value| IlstData {
            country_indicator: 0,
            language_indicator: 0,
            value,
        };
        let utf8 = |text: &str| value(IlstDataValue::Utf8(text.into()));
        assert_eq!(
            decoded,
            Ilst {
                mdta: vec![(1, vec![utf8("A"), utf8("B")])],
                items: vec![
                    (FourCC::new(b"\xa9ART"), vec![utf8("Alice"), utf8("Bob")]),
                    (
                        FourCC::new(b"\xa9cmt"),
                        vec![
                            value(IlstDataValue::BeSignedInt(5)),
                            value(IlstDataValue::BeSignedInt(6))
                        ]
                    ),
                    (
                        FourCC::new(b"desc"),
                        vec![utf8("First"), utf8(&"B".repeat(120))]
                    ),
                    (
                        FourCC::new(b"cprt"),
                        vec![value(IlstDataValue::Utf16("x".into()))]
                    ),
                ],
                ..Default::default()
            }
        );

        let mut reencoded = Vec::new();
        decoded.encode(&mut reencoded).unwrap();
        assert_eq!(reencoded, encoded);
    }

    // A `data` atom holding `value` under `type_indicator`, of no country or
    // language.
    fn data_atom(type_indicator: u32, value: &[u8]) -> Vec<u8> {
        let mut body = type_indicator.to_be_bytes().to_vec();
        body.extend_from_slice(&0u32.to_be_bytes()); // country + language
        body.extend_from_slice(value);
        atom_box(b"data", &body)
    }

    fn untyped(value: IlstDataValue) -> IlstData {
        IlstData {
            country_indicator: 0,
            language_indicator: 0,
            value,
        }
    }

    // An item of a kind this crate types that does not type is kept with the
    // untyped items, its values as they read: a `name` or `day ` wrapping a
    // `data` atom, as iTunes items do and as libavformat reads them (this crate
    // types these two as bare strings), and a `desc`, `©too` or `cprt` whose
    // value is not UTF-8 text. None of them fails the `ilst`, and the item
    // after them still reads.
    #[test]
    fn test_ilst_untypeable_items() {
        let latin1 = b"Caf\xe9";
        let lone_surrogate = [0xd8, 0x00];
        let encoded = atom_box(
            b"ilst",
            &[
                atom_box(b"name", &data_atom(1, b"Harbour")),
                atom_box(b"day ", &data_atom(1, b"2026")),
                atom_box(b"desc", &data_atom(1, latin1)),
                atom_box(b"\xa9too", &data_atom(2, &lone_surrogate)),
                atom_box(b"cprt", &data_atom(21, &[7])),
                atom_box(b"\xa9nam", &data_atom(1, b"Title")),
            ]
            .concat(),
        );

        let decoded = Ilst::decode(&mut encoded.as_slice()).unwrap();
        let utf8 = |text: &str| untyped(IlstDataValue::Utf8(text.into()));
        assert_eq!(
            decoded,
            Ilst {
                items: vec![
                    (FourCC::new(b"name"), vec![utf8("Harbour")]),
                    (FourCC::new(b"day "), vec![utf8("2026")]),
                    (
                        FourCC::new(b"desc"),
                        vec![untyped(IlstDataValue::Unknown(1, latin1.to_vec()))]
                    ),
                    (
                        FourCC::new(b"\xa9too"),
                        vec![untyped(IlstDataValue::Unknown(2, lone_surrogate.to_vec()))]
                    ),
                    (
                        FourCC::new(b"cprt"),
                        vec![untyped(IlstDataValue::BeSignedInt(7))]
                    ),
                    (FourCC::new(b"\xa9nam"), vec![utf8("Title")]),
                ],
                ..Default::default()
            }
        );

        let mut reencoded = Vec::new();
        decoded.encode(&mut reencoded).unwrap();
        assert_eq!(reencoded, encoded);
    }

    // A bare `name` string that is not UTF-8 is neither a `Name` nor an item
    // wrapping a `data` atom: it goes where an unknown item goes (an error in
    // strict/test builds, a warning otherwise), not out of the `ilst` as an
    // invalid string.
    #[test]
    fn test_ilst_unreadable_bare_name() {
        let encoded = atom_box(b"ilst", &atom_box(b"name", b"Caf\xe9\0"));
        match Ilst::decode(&mut encoded.as_slice()) {
            Err(Error::UnexpectedBox(kind)) => assert_eq!(kind, Name::KIND),
            other => panic!("expected UnexpectedBox(name), got {other:?}"),
        }
    }

    // A typed field holds the first item of its kind that types. A later one
    // that wraps `data` atoms is kept with the untyped items, in file order,
    // rather than replacing it: libavformat appends a repeated item's values
    // to the first one's.
    #[test]
    fn test_ilst_repeated_typed_items() {
        let jpeg = [0xff, 0xd8, 0xff, 0xd9];
        let png = [0x89, b'P', b'N', b'G'];
        let first_covr = data_atom(13, &jpeg);
        let encoded = atom_box(
            b"ilst",
            &[
                atom_box(b"desc", &data_atom(1, b"First")),
                atom_box(b"\xa9too", &data_atom(1, b"Lavf61.7.100")),
                atom_box(b"desc", &data_atom(1, b"Second")),
                atom_box(b"covr", &first_covr),
                atom_box(b"covr", &data_atom(14, &png)),
                atom_box(b"\xa9too", &data_atom(1, b"HandBrake 1.8.0")),
            ]
            .concat(),
        );

        let decoded = Ilst::decode(&mut encoded.as_slice()).unwrap();
        let text = |text: &str| untyped(IlstDataValue::Utf8(text.into()));
        assert_eq!(
            decoded,
            Ilst {
                covr: Some(Covr(first_covr)),
                desc: Some(Desc {
                    country_indicator: 0,
                    language_indicator: 0,
                    text: "First".into(),
                }),
                ctoo: Some(Tool {
                    country_indicator: 0,
                    language_indicator: 0,
                    text: "Lavf61.7.100".into(),
                }),
                items: vec![
                    (FourCC::new(b"desc"), vec![text("Second")]),
                    (
                        FourCC::new(b"covr"),
                        vec![untyped(IlstDataValue::Png(png.to_vec()))]
                    ),
                    (FourCC::new(b"\xa9too"), vec![text("HandBrake 1.8.0")]),
                ],
                ..Default::default()
            }
        );

        // Each item is written back; the typed fields come first.
        let mut reencoded = Vec::new();
        decoded.encode(&mut reencoded).unwrap();
        assert_eq!(Ilst::decode(&mut reencoded.as_slice()).unwrap(), decoded);
    }

    // A repeat of a typed kind that wraps no `data` atom (a second bare `name`
    // or `day ` string, a second raw `covr`) cannot be kept with the untyped
    // items: it goes where an unknown item goes (an error in strict/test
    // builds, a warning otherwise).
    #[test]
    fn test_ilst_repeated_bare_items() {
        let cases: [(&[u8; 4], &[u8]); 3] = [
            (b"name", b"Harbour\0"),
            (b"day ", b"2026\0"),
            (b"covr", &[0xff, 0xd8, 0xff, 0xd9]),
        ];
        for (kind, body) in cases {
            let item = atom_box(kind, body);
            let encoded = atom_box(b"ilst", &[item.as_slice(), &item].concat());
            match Ilst::decode(&mut encoded.as_slice()) {
                Err(Error::UnexpectedBox(atom)) => assert_eq!(atom, FourCC::new(kind)),
                other => panic!("expected UnexpectedBox({kind:?}), got {other:?}"),
            }
        }
    }

    // A movie whose `udta/meta/ilst` holds items that do not type reads whole,
    // with those items kept: the same items this crate writes for them.
    #[test]
    fn test_moov_with_untypeable_ilst_items() {
        let ilst = Ilst {
            items: vec![
                (
                    FourCC::new(b"name"),
                    vec![untyped(IlstDataValue::Utf8("Harbour".into()))],
                ),
                (
                    FourCC::new(b"desc"),
                    vec![untyped(IlstDataValue::Unknown(1, b"Caf\xe9".to_vec()))],
                ),
            ],
            ..Default::default()
        };
        let mut meta = Meta {
            hdlr: Hdlr {
                handler: FourCC::new(b"mdir"),
                name: "".into(),
                component_type: None,
            },
            items: vec![],
        };
        meta.push(ilst.clone());
        let moov = Moov {
            mvhd: Mvhd {
                timescale: 1000,
                next_track_id: 1,
                ..Default::default()
            },
            udta: Some(Udta {
                meta: Some(meta),
                ..Default::default()
            }),
            ..Default::default()
        };

        let mut encoded = Vec::new();
        moov.encode(&mut encoded).unwrap();
        let decoded = Moov::decode(&mut encoded.as_slice()).unwrap();
        let meta = decoded.udta.and_then(|udta| udta.meta).unwrap();
        assert_eq!(meta.get::<Ilst>(), Some(&ilst));
    }

    // An item that wraps no `data` atom (a `----` freeform item: `mean`,
    // `name`, `data`) still goes through decode_unknown (an error in
    // strict/test builds, a warning otherwise).
    #[test]
    fn test_ilst_unknown_item() {
        let mut mean = 0u32.to_be_bytes().to_vec();
        mean.extend_from_slice(b"com.apple.iTunes");
        let freeform = atom_box(b"----", &atom_box(b"mean", &mean));
        let encoded = atom_box(b"ilst", &freeform);

        let result = Ilst::decode(&mut encoded.as_slice());
        match result {
            Err(Error::UnexpectedBox(kind)) => assert_eq!(kind, FourCC::new(b"----")),
            other => panic!("expected UnexpectedBox, got {other:?}"),
        }
    }

    // Apple's `mdta`-keyed metadata scheme (used alongside a sibling `keys`
    // box) keys each `ilst` item by a raw 1-based index rather than a
    // well-known fourcc — the index is stored where a fourcc would be, with
    // its top byte zero (so it can't collide with a real ASCII/Latin1 tag).
    #[test]
    fn test_ilst_mdta_item() {
        let mut data_body = Vec::new();
        data_body.extend_from_slice(&1u32.to_be_bytes()); // type indicator: UTF-8
        data_body.extend_from_slice(&0u32.to_be_bytes()); // country + language
        data_body.extend_from_slice(b"Apple");
        let item = atom_box(&1u32.to_be_bytes(), &atom_box(b"data", &data_body));
        let encoded = atom_box(b"ilst", &item);

        let decoded = Ilst::decode(&mut encoded.as_slice()).unwrap();
        assert_eq!(
            decoded,
            Ilst {
                mdta: vec![(
                    1,
                    vec![IlstData {
                        country_indicator: 0,
                        language_indicator: 0,
                        value: IlstDataValue::Utf8("Apple".into()),
                    }]
                )],
                ..Default::default()
            }
        );

        // The encoder writes the same long-style `data` layout back.
        let mut reencoded = Vec::new();
        decoded.encode(&mut reencoded).unwrap();
        assert_eq!(reencoded, encoded);
    }

    // An out-of-range index would encode as a FourCC whose top byte is
    // nonzero, which could look like (or collide with) a real ASCII/Latin1
    // tag on the next decode -- reject it instead of silently corrupting it.
    #[test]
    fn test_ilst_mdta_index_out_of_range_rejected() {
        let ilst = Ilst {
            mdta: vec![(
                MDTA_INDEX_LIMIT,
                vec![IlstData {
                    country_indicator: 0,
                    language_indicator: 0,
                    value: IlstDataValue::Utf8("Apple".into()),
                }],
            )],
            ..Default::default()
        };

        let mut buf = Vec::new();
        let err = ilst.encode(&mut buf).unwrap_err();
        assert!(matches!(err, Error::Unsupported(_)));
    }
}
