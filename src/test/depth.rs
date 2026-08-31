//! The nesting limit that keeps a recursive descent over untrusted boxes from
//! becoming a recursive descent off the end of the stack.
//!
//! A container's payload is more containers, so decoding recurses as deep as
//! the file says. One FourCC nested inside itself is 8 bytes per level: without
//! a limit, a few hundred bytes of input abort the process with a stack
//! overflow, which no `Result` can catch.

use crate::*;

/// `n` nested `moov` boxes — 8 bytes each, the smallest self-recursive input.
fn nested_moov(n: usize) -> Vec<u8> {
    let mut buf: Vec<u8> = vec![0, 0, 0, 8, b'm', b'o', b'o', b'v'];
    for _ in 1..n {
        let mut outer = ((buf.len() + 8) as u32).to_be_bytes().to_vec();
        outer.extend_from_slice(b"moov");
        outer.extend_from_slice(&buf);
        buf = outer;
    }
    buf
}

/// The whole point: a typed error, not an abort. 64 levels is 512 bytes.
#[test]
fn nesting_past_the_limit_is_an_error_not_a_stack_overflow() {
    let bytes = nested_moov(64);
    assert_eq!(bytes.len(), 64 * 8);
    assert!(matches!(
        Any::decode(&mut bytes.as_slice()),
        Err(Error::TooDeep(kind)) if kind == Moov::KIND
    ));
}

/// Depth is counted, not estimated: a chain one level longer than the limit is
/// refused and one exactly at the limit is not. `moov` requires an `mvhd`, so
/// the accepted case still fails — with `MissingBox`, having descended the
/// whole way.
#[test]
fn the_limit_is_exact() {
    let at = nested_moov(MAX_ATOM_DEPTH as usize);
    assert!(matches!(
        Any::decode(&mut at.as_slice()),
        Err(Error::MissingBox(kind)) if kind == Mvhd::KIND
    ));

    let past = nested_moov(MAX_ATOM_DEPTH as usize + 1);
    assert!(matches!(
        Any::decode(&mut past.as_slice()),
        Err(Error::TooDeep(_))
    ));
}

/// The counter is released on the way back out, including through the `?` that
/// carries an error up. A leak would make each successive decode on this thread
/// start deeper than the last, so a run of them is what proves it.
#[test]
fn the_counter_unwinds_on_error() {
    let past = nested_moov(MAX_ATOM_DEPTH as usize + 1);
    for _ in 0..8 {
        assert!(matches!(
            Any::decode(&mut past.as_slice()),
            Err(Error::TooDeep(_))
        ));
    }

    // Still able to decode a box at the top level afterwards.
    let ftyp = b"\0\0\0\x14ftypiso6\0\0\x02\0mp41";
    assert!(matches!(
        Any::decode(&mut ftyp.as_slice()),
        Ok(Any::Ftyp(_))
    ));
}

/// The limit clears the deepest chain the crate models — `moov` / `trak` /
/// `mdia` / `minf` / `stbl` / `stsd` / sample entry / codec configuration —
/// which is the case that decides whether the number is usable at all.
#[test]
fn a_real_hierarchy_fits() {
    let mut moov = Vec::new();
    Moov {
        mvhd: Mvhd::default(),
        trak: vec![Trak {
            tkhd: Tkhd::default(),
            mdia: Mdia {
                minf: Minf {
                    stbl: Stbl {
                        stsd: Stsd {
                            codecs: vec![Avc1 {
                                avcc: Avcc {
                                    configuration_version: 1,
                                    length_size: 4,
                                    ..Default::default()
                                },
                                ..Default::default()
                            }
                            .into()],
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        }],
        ..Default::default()
    }
    .encode(&mut moov)
    .unwrap();

    let decoded = Moov::decode(&mut moov.as_slice()).expect("eight levels is not too deep");
    assert!(matches!(
        decoded.trak[0].mdia.minf.stbl.stsd.codecs[0],
        Codec::Avc1(_)
    ));
}
