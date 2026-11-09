//! A `meta` or `udta` that does not decode, or a second one, costs a `moov` or
//! a `trak` only itself: it is skipped and the first kept. This is the
//! library as a dependency sees it, so it runs where `skip_malformed` skips —
//! not under `strict`, where the error stands, nor in the crate's own unit
//! tests, which run as `strict` does.
#![cfg(not(feature = "strict"))]

use mp4_atom::{Decode, Encode, Moov, Udta};

/// A fragmented H.264 + AAC file whose `moov` holds two `trak`s and a
/// `udta/meta` (`src/test/bbb.rs`).
const BBB: &[u8] = include_bytes!("../src/test/bbb.mp4");

/// The 32-bit box at `offset` of `bytes`: its kind and its end.
fn box_at(bytes: &[u8], offset: usize) -> ([u8; 4], usize) {
    let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
    (
        bytes[offset + 4..offset + 8].try_into().unwrap(),
        offset + size,
    )
}

/// Where the first box of each kind of `path` starts, walking down from the
/// top level: one offset per kind.
fn find_path(bytes: &[u8], path: &[&[u8; 4]]) -> Vec<usize> {
    let (mut start, mut end) = (0, bytes.len());
    let mut found = Vec::new();
    for kind in path {
        let mut offset = start;
        loop {
            let (k, box_end) = box_at(bytes, offset);
            if &k == *kind {
                found.push(offset);
                (start, end) = (offset + 8, box_end);
                break;
            }
            offset = box_end;
            assert!(
                offset < end,
                "no {:?} in the file",
                std::str::from_utf8(*kind)
            );
        }
    }
    found
}

/// `bytes` with `child` appended to the end of the box `path` names, every box
/// on the path grown by its length.
fn insert(mut bytes: Vec<u8>, path: &[&[u8; 4]], child: &[u8]) -> Vec<u8> {
    let offsets = find_path(&bytes, path);
    let target_end = box_at(&bytes, *offsets.last().unwrap()).1;
    bytes.splice(target_end..target_end, child.iter().copied());
    for offset in offsets {
        let size =
            u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) + child.len() as u32;
        bytes[offset..offset + 4].copy_from_slice(&size.to_be_bytes());
    }
    bytes
}

fn moov_of(bytes: &[u8]) -> Moov {
    let moov = find_path(bytes, &[b"moov"])[0];
    Moov::decode(&mut &bytes[moov..box_at(bytes, moov).1]).expect("the moov decodes")
}

fn udta_box(udta: &Udta) -> Vec<u8> {
    let mut bytes = Vec::new();
    udta.encode(&mut bytes).unwrap();
    bytes
}

/// A `meta` too short for its version and flags.
const SHORT_META: &[u8] = b"\x00\x00\x00\x0ameta\x00\x00";

#[test]
fn a_malformed_movie_meta_costs_only_itself() {
    let moov = moov_of(&insert(BBB.to_vec(), &[b"moov"], SHORT_META));
    assert_eq!(
        moov,
        moov_of(BBB),
        "the short meta is skipped, every other box read"
    );
}

#[test]
fn a_second_movie_udta_is_skipped_and_the_first_kept() {
    let moov = moov_of(&insert(
        BBB.to_vec(),
        &[b"moov"],
        &udta_box(&Udta::default()),
    ));
    assert_eq!(moov, moov_of(BBB), "the first udta stands");
}

/// A `udta` whose one child states more bytes than the `udta` holds: it cannot
/// be framed, so the whole `udta` is skipped. As the movie's second `udta` it
/// would be skipped unread, so here it is the track's first.
#[test]
fn a_udta_that_cannot_be_framed_costs_only_itself() {
    // `udta` of 20 bytes, then a `©nam` stating 64.
    let unframed = b"\x00\x00\x00\x14udta\x00\x00\x00\x40\xa9nam\x00\x02\x55\xc4";
    let moov = moov_of(&insert(BBB.to_vec(), &[b"moov", b"trak"], unframed));
    assert_eq!(moov, moov_of(BBB), "the track reads as it did without it");
}

/// A track's `udta`, then a `meta` too short for its fields and a second
/// `udta`: the track reads with its first `udta`.
#[test]
fn a_tracks_malformed_meta_and_second_udta_cost_only_themselves() {
    let tagged = Udta {
        text: vec![],
        ..Default::default()
    };
    let with_udta = insert(BBB.to_vec(), &[b"moov", b"trak"], &udta_box(&tagged));
    let expected = moov_of(&with_udta);
    assert_eq!(
        expected.trak[0].udta,
        Some(tagged),
        "the track's udta is read"
    );

    let bytes = insert(
        with_udta,
        &[b"moov", b"trak"],
        &[SHORT_META, &udta_box(&Udta::default())].concat(),
    );
    assert_eq!(
        moov_of(&bytes),
        expected,
        "the short meta and the second udta are skipped"
    );
}
