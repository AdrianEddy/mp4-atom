//! `sv3d` / `st3d` / `vexu` / `hfov` / `uuid` — the geometry boxes.
//!
//! Every layout here is pinned against the only complete public description of
//! the wire format, `libavformat/mov.c` (`mov_read_sv3d`, `mov_read_st3d`,
//! `mov_read_vexu`, `mov_read_vexu_proj`, `mov_read_eyes`, `mov_read_pack`,
//! `mov_read_hfov`), by asserting the exact bytes rather than only a round trip
//! — a round trip alone would pass just as happily on a layout of our own
//! invention.

use crate::*;

/// Build a box: 4-byte size, FourCC, body.
fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

/// Build a FullBox v0 flags 0.
fn full(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; 4];
    b.extend_from_slice(body);
    boxed(kind, &b)
}

fn concat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}

// ─────────────────────────────────────────────────────────────────────
// Google Spherical Video V2
// ─────────────────────────────────────────────────────────────────────

#[test]
fn sv3d_equi_full_sphere_bytes() {
    let svhd = full(b"svhd", b"oxivideo\0");
    let prhd = full(b"prhd", &concat(&[vec![0; 4], vec![0; 4], vec![0; 4]]));
    let equi = full(b"equi", &[0u8; 16]);
    let proj = boxed(b"proj", &concat(&[prhd, equi]));
    let sv3d = boxed(b"sv3d", &concat(&[svhd, proj]));

    let decoded = Sv3d::decode(&mut sv3d.as_slice()).expect("sv3d");
    let proj = decoded.proj.clone().expect("proj");
    assert_eq!(decoded.svhd.as_ref().unwrap().metadata_source, "oxivideo");
    assert_eq!(proj.prhd, Some(Prhd::default()));
    assert_eq!(
        proj.kind,
        Some(ProjKind::Equi(Equi {
            projection_bounds_top: 0,
            projection_bounds_bottom: 0,
            projection_bounds_left: 0,
            projection_bounds_right: 0,
        }))
    );

    let mut out = Vec::new();
    decoded.encode(&mut out).unwrap();
    assert_eq!(out, sv3d, "sv3d must round-trip byte-for-byte");
}

#[test]
fn sv3d_equi_bounds_are_top_bottom_left_right() {
    // Distinct values so a transposed field order cannot pass.
    let mut bounds = Vec::new();
    for v in [1u32, 2, 3, 4] {
        bounds.extend_from_slice(&v.to_be_bytes());
    }
    let svhd = full(b"svhd", b"\0");
    let prhd = full(b"prhd", &[0u8; 12]);
    let equi = full(b"equi", &bounds);
    let sv3d = boxed(
        b"sv3d",
        &concat(&[svhd, boxed(b"proj", &concat(&[prhd, equi]))]),
    );

    let decoded = Sv3d::decode(&mut sv3d.as_slice()).expect("sv3d");
    match decoded.proj.unwrap().kind {
        Some(ProjKind::Equi(e)) => {
            assert_eq!(e.projection_bounds_top, 1);
            assert_eq!(e.projection_bounds_bottom, 2);
            assert_eq!(e.projection_bounds_left, 3);
            assert_eq!(e.projection_bounds_right, 4);
        }
        other => panic!("expected equi, got {other:?}"),
    }
}

#[test]
fn sv3d_prhd_is_signed_16_16_degrees() {
    // yaw 90, pitch 45, roll -30, all 16.16.
    let mut pose = Vec::new();
    for v in [90i32 << 16, 45 << 16, -30 << 16] {
        pose.extend_from_slice(&v.to_be_bytes());
    }
    let sv3d = boxed(
        b"sv3d",
        &concat(&[
            full(b"svhd", b"\0"),
            boxed(
                b"proj",
                &concat(&[full(b"prhd", &pose), full(b"cbmp", &[0u8; 8])]),
            ),
        ]),
    );
    let proj = Sv3d::decode(&mut sv3d.as_slice())
        .expect("sv3d")
        .proj
        .expect("proj");
    let prhd = proj.prhd.expect("prhd");
    assert_eq!(prhd.pose_yaw_degrees, 90 << 16);
    assert_eq!(prhd.pose_pitch_degrees, 45 << 16);
    assert_eq!(prhd.pose_roll_degrees, -30 << 16);
    assert!(matches!(proj.kind, Some(ProjKind::Cbmp(_))));
}

#[test]
fn sv3d_cbmp_layout_and_padding() {
    let mut body = 0u32.to_be_bytes().to_vec();
    body.extend_from_slice(&7u32.to_be_bytes());
    let sv3d = boxed(
        b"sv3d",
        &concat(&[
            full(b"svhd", b"\0"),
            boxed(
                b"proj",
                &concat(&[full(b"prhd", &[0u8; 12]), full(b"cbmp", &body)]),
            ),
        ]),
    );
    let decoded = Sv3d::decode(&mut sv3d.as_slice()).expect("sv3d");
    assert_eq!(
        decoded.proj.as_ref().unwrap().kind,
        Some(ProjKind::Cbmp(Cbmp {
            layout: 0,
            padding: 7
        }))
    );
    let mut out = Vec::new();
    decoded.encode(&mut out).unwrap();
    assert_eq!(out, sv3d);
}

#[test]
fn sv3d_mshp_retains_the_payload() {
    let mut body = 0xDEAD_BEEFu32.to_be_bytes().to_vec();
    body.extend_from_slice(b"dfl8");
    body.extend_from_slice(&[9, 8, 7, 6, 5]);
    let sv3d = boxed(
        b"sv3d",
        &concat(&[
            full(b"svhd", b"\0"),
            boxed(
                b"proj",
                &concat(&[full(b"prhd", &[0u8; 12]), full(b"mshp", &body)]),
            ),
        ]),
    );
    let decoded = Sv3d::decode(&mut sv3d.as_slice()).expect("sv3d");
    match &decoded.proj.as_ref().unwrap().kind {
        Some(ProjKind::Mshp(m)) => {
            assert_eq!(m.crc, 0xDEAD_BEEF);
            assert_eq!(m.encoding_four_cc, FourCC::new(b"dfl8"));
            assert_eq!(m.data, vec![9, 8, 7, 6, 5]);
        }
        other => panic!("expected mshp, got {other:?}"),
    }
    let mut out = Vec::new();
    decoded.encode(&mut out).unwrap();
    assert_eq!(out, sv3d, "the mesh payload must survive verbatim");
}

/// `mov_read_sv3d` logs "Unknown projection type" and `return 0`s, so a `proj`
/// naming a projection this crate does not model must not fail the file. The
/// box is kept verbatim — Google Spherical Video V2 says the projection set is
/// extensible, so this is a later revision rather than corruption — and
/// re-encodes byte-for-byte.
#[test]
fn sv3d_with_an_unmodelled_projection_is_carried_verbatim() {
    let sv3d = boxed(
        b"sv3d",
        &concat(&[
            full(b"svhd", b"\0"),
            boxed(
                b"proj",
                &concat(&[full(b"prhd", &[0u8; 12]), full(b"fish", &[1, 2, 3, 4])]),
            ),
        ]),
    );

    let decoded =
        Sv3d::decode(&mut sv3d.as_slice()).expect("an unmodelled projection is not an error");
    assert_eq!(
        decoded.proj.as_ref().unwrap().kind,
        // The body is everything after the 8-byte header: the FullBox
        // version/flags word, then the payload.
        Some(ProjKind::Unknown(
            FourCC::new(b"fish"),
            vec![0, 0, 0, 0, 1, 2, 3, 4]
        ))
    );

    let mut out = Vec::new();
    decoded.encode(&mut out).unwrap();
    assert_eq!(out, sv3d, "an unmodelled projection must survive verbatim");
}

/// The spec requires a projection data box; `mov_read_sv3d` still `return 0`s
/// without one, so `kind` is simply `None` and nothing is invented.
#[test]
fn sv3d_without_a_projection_data_box_is_tolerated() {
    let sv3d = boxed(
        b"sv3d",
        &concat(&[
            full(b"svhd", b"\0"),
            boxed(b"proj", &full(b"prhd", &[0u8; 12])),
        ]),
    );
    let decoded =
        Sv3d::decode(&mut sv3d.as_slice()).expect("a projection-less proj is not an error");
    assert_eq!(decoded.proj.as_ref().unwrap().kind, None);

    let mut out = Vec::new();
    decoded.encode(&mut out).unwrap();
    assert_eq!(out, sv3d, "nothing is invented to fill the gap");
}

/// "Missing spherical video header" is a `return 0` in `mov_read_sv3d`: ffmpeg
/// reads the file, so refusing it would leave two readers disagreeing about the
/// same bytes. Likewise "Missing projection header box" for `prhd`.
#[test]
fn sv3d_without_svhd_or_prhd_is_tolerated() {
    let sv3d = boxed(b"sv3d", &boxed(b"proj", &full(b"equi", &[0u8; 16])));
    let decoded = Sv3d::decode(&mut sv3d.as_slice()).expect("a headerless sv3d is not an error");
    assert_eq!(decoded.svhd, None);
    let proj = decoded.proj.as_ref().unwrap();
    assert_eq!(proj.prhd, None, "absent, not defaulted into existence");
    assert_eq!(
        proj.prhd.unwrap_or_default(),
        Prhd::default(),
        "the pose a missing prhd means is the identity"
    );
    assert!(matches!(proj.kind, Some(ProjKind::Equi(_))));

    let mut out = Vec::new();
    decoded.encode(&mut out).unwrap();
    assert_eq!(out, sv3d, "no svhd or prhd is invented on the way out");
}

/// An `sv3d` with no `proj` at all — the third `return 0` in `mov_read_sv3d`.
#[test]
fn sv3d_without_proj_is_tolerated() {
    let sv3d = boxed(b"sv3d", &full(b"svhd", b"x\0"));
    let decoded = Sv3d::decode(&mut sv3d.as_slice()).expect("a proj-less sv3d is not an error");
    assert_eq!(decoded.proj, None);
    assert_eq!(decoded.svhd.as_ref().unwrap().metadata_source, "x");
}

/// Padding after the last child of `sv3d` or `proj`, which every other
/// container atom in the crate already tolerates. Without the drain the
/// leftover bytes are an `UnderDecode` that fails the sample entry, and with it
/// the whole file.
#[test]
fn sv3d_tolerates_sub_header_trailing_padding() {
    let proj = {
        let mut b = concat(&[full(b"prhd", &[0u8; 12]), full(b"equi", &[0u8; 16])]);
        b.extend_from_slice(&[0, 0, 0, 0]);
        boxed(b"proj", &b)
    };
    let mut body = concat(&[full(b"svhd", b"\0"), proj]);
    body.extend_from_slice(&[0, 0, 0, 0]);
    let sv3d = boxed(b"sv3d", &body);

    let decoded = Sv3d::decode(&mut sv3d.as_slice()).expect("padding must be tolerated");
    assert!(matches!(
        decoded.proj.as_ref().unwrap().kind,
        Some(ProjKind::Equi(_))
    ));
}

/// The same for every `vexu` container: `vexu`, `vexu/proj`, `eyes`, `cams`,
/// `cmfy` and `pack` each drain a sub-header remainder.
#[test]
fn vexu_containers_tolerate_sub_header_trailing_padding() {
    fn padded(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut b = body.to_vec();
        b.extend_from_slice(&[0, 0, 0]);
        boxed(kind, &b)
    }

    let proj = padded(b"proj", &full(b"prji", b"equi"));
    let cams = padded(b"cams", &full(b"blin", &65_000u32.to_be_bytes()));
    let cmfy = padded(b"cmfy", &full(b"dadj", &0i32.to_be_bytes()));
    let eyes = padded(b"eyes", &concat(&[full(b"stri", &[0b0011]), cams, cmfy]));
    let pack = padded(b"pack", &full(b"pkin", b"side"));
    let vexu = padded(b"vexu", &concat(&[proj, eyes, pack]));

    let decoded = Vexu::decode(&mut vexu.as_slice()).expect("padding must be tolerated");
    assert_eq!(decoded.proj.unwrap().prji.projection, Prji::EQUIRECTANGULAR);
    let eyes = decoded.eyes.unwrap();
    assert_eq!(eyes.cams.unwrap().blin.baseline, 65_000);
    assert_eq!(eyes.cmfy.unwrap().dadj.disparity_adjustment, 0);
    assert_eq!(
        decoded.pack.unwrap().pkin.unwrap().packing,
        Pkin::SIDE_BY_SIDE
    );
}

#[test]
fn sv3d_truncated_child_is_an_error_not_a_panic() {
    let mut sv3d = boxed(
        b"sv3d",
        &concat(&[
            full(b"svhd", b"\0"),
            boxed(
                b"proj",
                &concat(&[full(b"prhd", &[0u8; 12]), full(b"equi", &[0u8; 16])]),
            ),
        ]),
    );
    // Chop the last eight bytes off the equi payload while leaving the outer
    // sizes claiming they are there.
    sv3d.truncate(sv3d.len() - 8);
    assert!(Sv3d::decode(&mut sv3d.as_slice()).is_err());
}

#[test]
fn st3d_carries_the_mode_byte_verbatim() {
    for mode in [0u8, 1, 2, 3, 4, 255] {
        let bytes = full(b"st3d", &[mode]);
        let decoded = St3d::decode(&mut bytes.as_slice()).expect("st3d");
        assert_eq!(decoded.stereo_mode, mode);
        let mut out = Vec::new();
        decoded.encode(&mut out).unwrap();
        assert_eq!(out, bytes);
    }
}

// ─────────────────────────────────────────────────────────────────────
// Apple Projected Media Profile
// ─────────────────────────────────────────────────────────────────────

#[test]
fn vexu_full_shape_bytes() {
    let prji = full(b"prji", b"hequ");
    let proj = boxed(b"proj", &prji);
    // stri: eye_views_reversed | has_left | has_right
    let stri = full(b"stri", &[0b1011]);
    let hero = full(b"hero", &[Hero::RIGHT]);
    let blin = full(b"blin", &65_000u32.to_be_bytes());
    let cams = boxed(b"cams", &blin);
    let dadj = full(b"dadj", &(-2500i32).to_be_bytes());
    let cmfy = boxed(b"cmfy", &dadj);
    let eyes = boxed(b"eyes", &concat(&[stri, hero, cams, cmfy]));
    let pkin = full(b"pkin", b"side");
    let pack = boxed(b"pack", &pkin);
    let vexu = boxed(b"vexu", &concat(&[proj, eyes, pack]));

    // The `cams` / `cmfy` child sizes ffmpeg hard-asserts: 24 outer, 16 inner.
    assert_eq!(boxed(b"cams", &full(b"blin", &[0u8; 4])).len(), 24);
    assert_eq!(full(b"blin", &[0u8; 4]).len(), 16);
    // ... and the 13-byte `stri` / `hero`, 16-byte `prji` / `pkin`.
    assert_eq!(full(b"stri", &[0]).len(), 13);
    assert_eq!(full(b"prji", b"rect").len(), 16);

    let decoded = Vexu::decode(&mut vexu.as_slice()).expect("vexu");
    assert_eq!(
        decoded.proj.unwrap().prji.projection,
        Prji::HALF_EQUIRECTANGULAR
    );
    let eyes = decoded.eyes.unwrap();
    let stri = eyes.stri.unwrap();
    assert!(stri.eye_views_reversed);
    assert!(!stri.has_additional_views);
    assert!(stri.has_right_eye_view);
    assert!(stri.has_left_eye_view);
    assert_eq!(eyes.hero.unwrap().hero_eye, Hero::RIGHT);
    assert_eq!(eyes.cams.unwrap().blin.baseline, 65_000);
    assert_eq!(eyes.cmfy.unwrap().dadj.disparity_adjustment, -2500);
    assert_eq!(
        decoded.pack.unwrap().pkin.unwrap().packing,
        Pkin::SIDE_BY_SIDE
    );

    let mut out = Vec::new();
    decoded.encode(&mut out).unwrap();
    assert_eq!(out, vexu, "vexu must round-trip byte-for-byte");
}

#[test]
fn stri_bit_positions() {
    // Bit 3 reversed, bit 2 additional, bit 1 right, bit 0 left.
    for (bits, want) in [
        (0b0001u8, (false, false, false, true)),
        (0b0010, (false, false, true, false)),
        (0b0100, (false, true, false, false)),
        (0b1000, (true, false, false, false)),
    ] {
        let bytes = full(b"stri", &[bits]);
        let s = Stri::decode(&mut bytes.as_slice()).unwrap();
        assert_eq!(
            (
                s.eye_views_reversed,
                s.has_additional_views,
                s.has_right_eye_view,
                s.has_left_eye_view
            ),
            want,
            "bits {bits:#06b}"
        );
        let mut out = Vec::new();
        s.encode(&mut out).unwrap();
        assert_eq!(out, bytes);
    }
}

#[test]
fn hfov_is_a_plain_box_of_thousandths() {
    let bytes = boxed(b"hfov", &180_000u32.to_be_bytes());
    assert_eq!(bytes.len(), 12, "hfov body is exactly four bytes");
    let decoded = Hfov::decode(&mut bytes.as_slice()).expect("hfov");
    assert_eq!(decoded.horizontal_field_of_view, 180_000);
    assert_eq!(
        decoded.horizontal_field_of_view / Hfov::DENOMINATOR,
        180,
        "180 degrees"
    );
    let mut out = Vec::new();
    decoded.encode(&mut out).unwrap();
    assert_eq!(out, bytes);
}

#[test]
fn vexu_missing_prji_is_a_missing_box() {
    let vexu = boxed(b"vexu", &boxed(b"proj", &[]));
    assert!(matches!(
        Vexu::decode(&mut vexu.as_slice()),
        Err(Error::MissingBox(k)) if k == Prji::KIND
    ));
}

// ─────────────────────────────────────────────────────────────────────
// Sample entries
// ─────────────────────────────────────────────────────────────────────

/// The 78-byte `VisualSampleEntry` fixed header for a `w × h` picture.
fn visual_header(w: u16, h: u16) -> Vec<u8> {
    let mut v = Vec::new();
    Visual {
        data_reference_index: 1,
        width: w,
        height: h,
        ..Default::default()
    }
    .encode(&mut v)
    .unwrap();
    v
}

fn sample_sv3d() -> Vec<u8> {
    boxed(
        b"sv3d",
        &concat(&[
            full(b"svhd", b"t\0"),
            boxed(
                b"proj",
                &concat(&[full(b"prhd", &[0u8; 12]), full(b"equi", &[0u8; 16])]),
            ),
        ]),
    )
}

#[test]
fn avc1_carries_the_geometry_children() {
    let mut avcc = Vec::new();
    Avcc {
        configuration_version: 1,
        avc_profile_indication: 100,
        profile_compatibility: 0,
        avc_level_indication: 13,
        length_size: 4,
        sequence_parameter_sets: vec![vec![0x67, 0x64, 0x00, 0x0D]],
        picture_parameter_sets: vec![vec![0x68, 0xEB, 0xE3, 0xCB]],
        ..Default::default()
    }
    .encode(&mut avcc)
    .unwrap();

    let entry = boxed(
        b"avc1",
        &concat(&[
            visual_header(320, 240),
            avcc,
            sample_sv3d(),
            full(b"st3d", &[2]),
            boxed(b"vexu", &boxed(b"proj", &full(b"prji", b"equi"))),
            boxed(b"hfov", &360_000u32.to_be_bytes()),
        ]),
    );

    let decoded = Avc1::decode(&mut entry.as_slice()).expect("avc1");
    assert!(decoded.sv3d.is_some());
    assert_eq!(decoded.st3d.unwrap().stereo_mode, St3d::LEFT_RIGHT);
    assert_eq!(
        decoded.vexu.as_ref().unwrap().proj.unwrap().prji.projection,
        Prji::EQUIRECTANGULAR
    );
    assert_eq!(decoded.hfov.unwrap().horizontal_field_of_view, 360_000);

    let mut out = Vec::new();
    decoded.encode(&mut out).unwrap();
    assert_eq!(out, entry);
}

// ─────────────────────────────────────────────────────────────────────
// uuid
// ─────────────────────────────────────────────────────────────────────

/// Spherical Video V1's user type.
const GSPHERICAL: ExtendedType = ExtendedType::new(&[
    0xff, 0xcc, 0x82, 0x63, 0xf8, 0x55, 0x4a, 0x93, 0x88, 0x14, 0x58, 0x7a, 0x02, 0x52, 0x1f, 0xdd,
]);

#[test]
fn uuid_round_trips_usertype_and_body() {
    let xml = b"<rdf:SphericalVideo/>";
    let mut body = GSPHERICAL.as_ref().to_vec();
    body.extend_from_slice(xml);
    let bytes = boxed(b"uuid", &body);

    let decoded = Uuid::decode(&mut bytes.as_slice()).expect("uuid");
    assert_eq!(decoded, Uuid::Unknown(GSPHERICAL, xml.to_vec()));

    let mut out = Vec::new();
    decoded.encode(&mut out).unwrap();
    assert_eq!(out, bytes);
}

#[test]
fn uuid_shorter_than_its_usertype_errors() {
    let bytes = boxed(b"uuid", &GSPHERICAL.as_ref()[..10]);
    assert!(Uuid::decode(&mut bytes.as_slice()).is_err());
}

#[test]
fn trak_keeps_its_uuid_children_in_file_order() {
    let mut tkhd = Vec::new();
    Tkhd::default().encode(&mut tkhd).unwrap();
    let mut mdia = Vec::new();
    Mdia::default().encode(&mut mdia).unwrap();

    let mut a = GSPHERICAL.as_ref().to_vec();
    a.extend_from_slice(b"first");
    let mut b = [0xAAu8; 16].to_vec();
    b.extend_from_slice(b"second");

    let trak = boxed(
        b"trak",
        &concat(&[tkhd, mdia, boxed(b"uuid", &a), boxed(b"uuid", &b)]),
    );

    let decoded = Trak::decode(&mut trak.as_slice()).expect("trak");
    assert_eq!(
        decoded.uuid,
        vec![
            Uuid::Unknown(GSPHERICAL, b"first".to_vec()),
            Uuid::Unknown([0xAA; 16].into(), b"second".to_vec()),
        ]
    );
}

// ─────────────────────────────────────────────────────────────────────
// Unmodelled sample entries
// ─────────────────────────────────────────────────────────────────────

/// Wrap sample-entry bytes in an `stsd` declaring `entries` of them, followed
/// by `trailing` bytes the entry count does not describe.
fn stsd_bytes(entries: u32, body: &[u8], trailing: &[u8]) -> Vec<u8> {
    let mut b = entries.to_be_bytes().to_vec();
    b.extend_from_slice(body);
    b.extend_from_slice(trailing);
    full(b"stsd", &b)
}

#[test]
fn unknown_sample_entry_exposes_its_visual_children() {
    // CineForm: a visual sample entry whose codec this crate does not model,
    // carrying the codec-independent geometry boxes.
    let entry = boxed(
        b"CFHD",
        &concat(&[
            visual_header(5760, 2880),
            sample_sv3d(),
            full(b"st3d", &[0]),
        ]),
    );
    let stsd = stsd_bytes(1, &entry, &[]);

    let decoded = Stsd::decode(&mut stsd.as_slice()).expect("stsd");
    assert_eq!(decoded.codecs.len(), 1);
    let Codec::Unknown(kind, body) = &decoded.codecs[0] else {
        panic!("expected an unknown entry, got {:?}", decoded.codecs[0]);
    };
    assert_eq!(*kind, FourCC::new(b"CFHD"));
    // The retained body is everything after the 8-byte box header.
    assert_eq!(body.as_slice(), &entry[8..]);

    let visual =
        UnknownVisual::decode(&mut body.as_slice()).expect("visual view of the CFHD entry");
    assert_eq!(visual.visual.width, 5760);
    assert_eq!(visual.visual.height, 2880);
    assert!(visual.sv3d.is_some());
    assert_eq!(visual.st3d.unwrap().stereo_mode, St3d::MONO);
    assert!(visual.other.is_empty());

    // …and the entry this crate cannot model is written back byte for byte, so
    // a `CFHD` track's `sv3d`/`st3d` survive a read → write cycle rather than
    // costing the whole `stsd`.
    let mut out = Vec::new();
    decoded
        .encode(&mut out)
        .expect("an unmodelled entry re-encodes verbatim");
    assert_eq!(out, stsd);
}

#[test]
fn unknown_visual_view_of_a_truncated_body_errors() {
    // A body far shorter than the 78-byte visual header.
    assert!(UnknownVisual::decode(&mut [0u8; 10].as_slice()).is_err());
}

#[test]
fn unknown_visual_keeps_unmodelled_children_in_file_order() {
    let entry = concat(&[
        visual_header(64, 64),
        boxed(b"gama", &0x0002_3333u32.to_be_bytes()),
        full(b"st3d", &[1]),
        boxed(b"fake", b"xyz"),
    ]);
    let visual = UnknownVisual::decode(&mut entry.as_slice()).expect("visual");
    assert_eq!(visual.st3d.unwrap().stereo_mode, St3d::TOP_BOTTOM);
    assert_eq!(visual.other.len(), 2);
    assert_eq!(visual.other[0].kind(), FourCC::new(b"gama"));
    assert_eq!(visual.other[1].kind(), FourCC::new(b"fake"));
}

#[test]
fn stsd_with_two_unknown_entries_keeps_both() {
    let first = boxed(b"CFHD", &visual_header(64, 64));
    let second = boxed(
        b"CFHD",
        &concat(&[visual_header(64, 64), sample_sv3d(), full(b"st3d", &[1])]),
    );
    let stsd = stsd_bytes(2, &concat(&[first, second]), &[]);

    let decoded = Stsd::decode(&mut stsd.as_slice()).expect("stsd");
    assert_eq!(decoded.codecs.len(), 2);
    let Codec::Unknown(_, a) = &decoded.codecs[0] else {
        panic!()
    };
    let Codec::Unknown(_, b) = &decoded.codecs[1] else {
        panic!()
    };
    assert!(UnknownVisual::decode(&mut a.as_slice())
        .unwrap()
        .sv3d
        .is_none());
    let b = UnknownVisual::decode(&mut b.as_slice()).unwrap();
    assert!(b.sv3d.is_some());
    assert_eq!(b.st3d.unwrap().stereo_mode, St3d::TOP_BOTTOM);
}

/// GoPro's CineForm MOV muxer writes the track's `colr` as a SIBLING of the
/// sample entry, leaving a whole box inside `stsd` that `entry_count = 1` does
/// not describe. `skip_trailing_padding`'s sub-header contract does not reach a
/// remainder that large, so `Stsd` drains it itself — otherwise the whole
/// `moov`, and therefore the whole file, fails to open.
#[test]
fn stsd_drains_a_whole_box_after_the_declared_entries() {
    let mut colr = Vec::new();
    Colr::Nclc {
        colour_primaries: 1,
        transfer_characteristics: 1,
        matrix_coefficients: 1,
    }
    .encode(&mut colr)
    .unwrap();
    assert!(colr.len() >= 8, "the remainder must be a whole box's worth");

    let entry = boxed(b"CFHD", &visual_header(1920, 1080));
    let stsd = stsd_bytes(1, &entry, &colr);

    let decoded = Stsd::decode(&mut stsd.as_slice()).expect("trailing sibling box must be drained");
    assert_eq!(decoded.codecs.len(), 1);
    let Codec::Unknown(kind, _) = &decoded.codecs[0] else {
        panic!("expected an unknown entry, got {:?}", decoded.codecs[0]);
    };
    assert_eq!(*kind, FourCC::new(b"CFHD"));
}

/// What that drain reports. A byte count alone does not say which box was
/// thrown away, and a sibling `sv3d` or `st3d` written past the entry count is
/// exactly what hides there. The walk runs on untrusted bytes, so it stops at
/// the first length that cannot be a box rather than looping or reading past
/// the end.
#[test]
fn stsd_names_the_boxes_it_drains() {
    let mut colr = Vec::new();
    Colr::Nclc {
        colour_primaries: 1,
        transfer_characteristics: 1,
        matrix_coefficients: 1,
    }
    .encode(&mut colr)
    .unwrap();

    let bytes = concat(&[colr, full(b"st3d", &[0])]);
    assert_eq!(
        trailing_kinds(&bytes),
        vec![FourCC::new(b"colr"), FourCC::new(b"st3d")]
    );

    // A length that cannot be a box ends the walk, after naming the box it was
    // found on.
    let mut broken = bytes.clone();
    broken.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    broken.extend_from_slice(b"sv3d");
    assert_eq!(
        trailing_kinds(&broken),
        vec![
            FourCC::new(b"colr"),
            FourCC::new(b"st3d"),
            FourCC::new(b"sv3d")
        ]
    );

    // A zero length would otherwise never advance.
    assert_eq!(
        trailing_kinds(&[0, 0, 0, 0, b'f', b'r', b'e', b'e', 0, 0]),
        vec![FourCC::new(b"free")]
    );
    // A remainder too short to hold a header names nothing.
    assert!(trailing_kinds(&[0, 0, 0]).is_empty());
}

/// The sub-header remainder every sample-entry decoder already tolerates,
/// here at the `stsd` level.
#[test]
fn stsd_tolerates_sub_header_trailing_padding() {
    let entry = boxed(b"CFHD", &visual_header(320, 240));
    let stsd = stsd_bytes(1, &entry, &[0, 0, 0, 0]);
    let decoded = Stsd::decode(&mut stsd.as_slice()).expect("padding must be tolerated");
    assert_eq!(decoded.codecs.len(), 1);
}
