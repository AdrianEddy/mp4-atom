use crate::*;

/// `vexu` — Apple's **Video Extended Usage** box, a child of a visual sample
/// entry. The container half of Apple Projected/Immersive Media Profile
/// (spatial video, APMP):
///
/// ```text
/// vexu (Box)
///   proj (Box)          -> prji (FullBox v0)  projection FourCC
///   eyes (Box)          -> stri | hero | cams{blin} | cmfy{dadj}
///   pack (Box)          -> pkin (FullBox v0)  frame-packing FourCC
/// ```
///
/// Layout and semantics are those `libavformat/mov.c` reads (`mov_read_vexu`,
/// `mov_read_vexu_proj`, `mov_read_eyes`, `mov_read_pack`) — the only complete
/// public description of the boxes.
///
/// The `hfov` box is a **sibling** of `vexu`, not a child: see [`Hfov`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Vexu {
    pub proj: Option<VexuProj>,
    pub eyes: Option<Eyes>,
    pub pack: Option<Pack>,
}

impl Atom for Vexu {
    const KIND: FourCC = FourCC::new(b"vexu");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut out = Vexu::default();

        while let Some(header) = Header::decode_maybe(buf)? {
            match header.kind {
                VexuProj::KIND => {
                    if out.proj.is_some() {
                        return Err(Error::DuplicateBox(VexuProj::KIND));
                    }
                    out.proj = Some(VexuProj::decode_atom(&header, buf)?);
                }
                Eyes::KIND => {
                    if out.eyes.is_some() {
                        return Err(Error::DuplicateBox(Eyes::KIND));
                    }
                    out.eyes = Some(Eyes::decode_atom(&header, buf)?);
                }
                Pack::KIND => {
                    if out.pack.is_some() {
                        return Err(Error::DuplicateBox(Pack::KIND));
                    }
                    out.pack = Some(Pack::decode_atom(&header, buf)?);
                }
                _ => {
                    let unknown = Any::decode_atom(&header, buf)?;
                    Self::decode_unknown(&unknown)?;
                }
            }
        }
        skip_trailing_padding(buf);

        Ok(out)
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.proj.encode(buf)?;
        self.eyes.encode(buf)?;
        self.pack.encode(buf)
    }
}

/// `vexu/proj` — the projection container. Distinct from the Google
/// [`Proj`](super::Proj), which shares the FourCC and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VexuProj {
    pub prji: Prji,
}

impl Atom for VexuProj {
    const KIND: FourCC = FourCC::new(b"proj");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut prji = None;
        while let Some(header) = Header::decode_maybe(buf)? {
            match header.kind {
                Prji::KIND => {
                    if prji.is_some() {
                        return Err(Error::DuplicateBox(Prji::KIND));
                    }
                    prji = Some(Prji::decode_atom(&header, buf)?);
                }
                _ => {
                    let unknown = Any::decode_atom(&header, buf)?;
                    Self::decode_unknown(&unknown)?;
                }
            }
        }
        skip_trailing_padding(buf);

        Ok(VexuProj {
            prji: prji.ok_or(Error::MissingBox(Prji::KIND))?,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.prji.encode(buf)
    }
}

/// `prji` — Projection Info. One FourCC naming the projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Prji {
    pub projection: FourCC,
}

impl Prji {
    /// Flat, rectangular 2D.
    pub const RECTILINEAR: FourCC = FourCC::new(b"rect");
    /// Full equirectangular.
    pub const EQUIRECTANGULAR: FourCC = FourCC::new(b"equi");
    /// 180° equirectangular.
    pub const HALF_EQUIRECTANGULAR: FourCC = FourCC::new(b"hequ");
    /// Fisheye.
    pub const FISHEYE: FourCC = FourCC::new(b"fish");
    /// Parametric immersive. Note the code is `prim`, not `para`.
    pub const PARAMETRIC_IMMERSIVE: FourCC = FourCC::new(b"prim");
}

impl Default for Prji {
    fn default() -> Self {
        Prji {
            projection: Prji::RECTILINEAR,
        }
    }
}

impl AtomExt for Prji {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"prji");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        Ok(Prji {
            projection: FourCC::decode(buf)?,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.projection.encode(buf)
    }
}

/// `eyes` — the stereo-view container inside [`Vexu`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Eyes {
    pub stri: Option<Stri>,
    pub hero: Option<Hero>,
    pub cams: Option<Cams>,
    pub cmfy: Option<Cmfy>,
}

impl Atom for Eyes {
    const KIND: FourCC = FourCC::new(b"eyes");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut out = Eyes::default();
        while let Some(header) = Header::decode_maybe(buf)? {
            match header.kind {
                Stri::KIND => {
                    if out.stri.is_some() {
                        return Err(Error::DuplicateBox(Stri::KIND));
                    }
                    out.stri = Some(Stri::decode_atom(&header, buf)?);
                }
                Hero::KIND => {
                    if out.hero.is_some() {
                        return Err(Error::DuplicateBox(Hero::KIND));
                    }
                    out.hero = Some(Hero::decode_atom(&header, buf)?);
                }
                Cams::KIND => {
                    if out.cams.is_some() {
                        return Err(Error::DuplicateBox(Cams::KIND));
                    }
                    out.cams = Some(Cams::decode_atom(&header, buf)?);
                }
                Cmfy::KIND => {
                    if out.cmfy.is_some() {
                        return Err(Error::DuplicateBox(Cmfy::KIND));
                    }
                    out.cmfy = Some(Cmfy::decode_atom(&header, buf)?);
                }
                _ => {
                    let unknown = Any::decode_atom(&header, buf)?;
                    Self::decode_unknown(&unknown)?;
                }
            }
        }
        skip_trailing_padding(buf);

        Ok(out)
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.stri.encode(buf)?;
        self.hero.encode(buf)?;
        self.cams.encode(buf)?;
        self.cmfy.encode(buf)
    }
}

/// `stri` — Stereo Information. One byte of flags saying which eye views the
/// frame carries and whether they are stored the other way round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Stri {
    /// Bit 3 — the stored order of the two views is reversed.
    pub eye_views_reversed: bool,
    /// Bit 2 — views beyond the two eyes are present.
    pub has_additional_views: bool,
    /// Bit 1.
    pub has_right_eye_view: bool,
    /// Bit 0.
    pub has_left_eye_view: bool,
}

impl AtomExt for Stri {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"stri");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        let bits = u8::decode(buf)?;
        Ok(Stri {
            eye_views_reversed: bits & 0b1000 != 0,
            has_additional_views: bits & 0b0100 != 0,
            has_right_eye_view: bits & 0b0010 != 0,
            has_left_eye_view: bits & 0b0001 != 0,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        let bits = (self.eye_views_reversed as u8) << 3
            | (self.has_additional_views as u8) << 2
            | (self.has_right_eye_view as u8) << 1
            | (self.has_left_eye_view as u8);
        bits.encode(buf)
    }
}

/// `hero` — which eye is the primary one when rendering in 2D:
/// `0` neither, `1` left, `2` right. Carried as the wire byte so an
/// unrecognised value survives the round trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Hero {
    pub hero_eye: u8,
}

impl Hero {
    pub const NONE: u8 = 0;
    pub const LEFT: u8 = 1;
    pub const RIGHT: u8 = 2;
}

impl AtomExt for Hero {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"hero");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        Ok(Hero {
            hero_eye: u8::decode(buf)?,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.hero_eye.encode(buf)
    }
}

/// `cams` — the camera-system container; carries the interocular baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Cams {
    pub blin: Blin,
}

impl Atom for Cams {
    const KIND: FourCC = FourCC::new(b"cams");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut blin = None;
        while let Some(header) = Header::decode_maybe(buf)? {
            match header.kind {
                Blin::KIND => {
                    if blin.is_some() {
                        return Err(Error::DuplicateBox(Blin::KIND));
                    }
                    blin = Some(Blin::decode_atom(&header, buf)?);
                }
                _ => {
                    let unknown = Any::decode_atom(&header, buf)?;
                    Self::decode_unknown(&unknown)?;
                }
            }
        }
        skip_trailing_padding(buf);

        Ok(Cams {
            blin: blin.ok_or(Error::MissingBox(Blin::KIND))?,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.blin.encode(buf)
    }
}

/// `blin` — the distance between the two lens centres, in **micrometres**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Blin {
    pub baseline: u32,
}

impl AtomExt for Blin {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"blin");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        Ok(Blin {
            baseline: u32::decode(buf)?,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.baseline.encode(buf)
    }
}

/// `cmfy` — the comfort container; carries the horizontal disparity adjustment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Cmfy {
    pub dadj: Dadj,
}

impl Atom for Cmfy {
    const KIND: FourCC = FourCC::new(b"cmfy");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut dadj = None;
        while let Some(header) = Header::decode_maybe(buf)? {
            match header.kind {
                Dadj::KIND => {
                    if dadj.is_some() {
                        return Err(Error::DuplicateBox(Dadj::KIND));
                    }
                    dadj = Some(Dadj::decode_atom(&header, buf)?);
                }
                _ => {
                    let unknown = Any::decode_atom(&header, buf)?;
                    Self::decode_unknown(&unknown)?;
                }
            }
        }
        skip_trailing_padding(buf);

        Ok(Cmfy {
            dadj: dadj.ok_or(Error::MissingBox(Dadj::KIND))?,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.dadj.encode(buf)
    }
}

/// `dadj` — horizontal disparity adjustment, in **ten-thousandths**: the value
/// divided by [`Dadj::DENOMINATOR`] is the −1.0..1.0 shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Dadj {
    pub disparity_adjustment: i32,
}

impl Dadj {
    /// The fixed denominator the field is expressed over.
    pub const DENOMINATOR: i32 = 10_000;
}

impl AtomExt for Dadj {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"dadj");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        Ok(Dadj {
            disparity_adjustment: i32::decode(buf)?,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.disparity_adjustment.encode(buf)
    }
}

/// `pack` — the frame-packing container inside [`Vexu`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Pack {
    pub pkin: Option<Pkin>,
}

impl Atom for Pack {
    const KIND: FourCC = FourCC::new(b"pack");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut out = Pack::default();
        while let Some(header) = Header::decode_maybe(buf)? {
            match header.kind {
                Pkin::KIND => {
                    if out.pkin.is_some() {
                        return Err(Error::DuplicateBox(Pkin::KIND));
                    }
                    out.pkin = Some(Pkin::decode_atom(&header, buf)?);
                }
                _ => {
                    let unknown = Any::decode_atom(&header, buf)?;
                    Self::decode_unknown(&unknown)?;
                }
            }
        }
        skip_trailing_padding(buf);

        Ok(out)
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.pkin.encode(buf)
    }
}

/// `pkin` — Packing Info. One FourCC naming how the two views are packed:
/// `side` side-by-side, `over` over-under, all-zero "stated elsewhere".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Pkin {
    pub packing: FourCC,
}

impl Pkin {
    pub const SIDE_BY_SIDE: FourCC = FourCC::new(b"side");
    pub const OVER_UNDER: FourCC = FourCC::new(b"over");
    /// All-zero: the packing is stated by another layer, not here.
    pub const UNSET: FourCC = FourCC::new(&[0, 0, 0, 0]);
}

impl Default for Pkin {
    fn default() -> Self {
        Pkin {
            packing: Pkin::UNSET,
        }
    }
}

impl AtomExt for Pkin {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"pkin");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        Ok(Pkin {
            packing: FourCC::decode(buf)?,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.packing.encode(buf)
    }
}

/// `hfov` — horizontal field of view in **thousandths of a degree**. A plain
/// box (no version/flags) holding one `u32`, and a sibling of [`Vexu`] inside
/// the visual sample entry rather than one of its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Hfov {
    pub horizontal_field_of_view: u32,
}

impl Hfov {
    /// The fixed denominator the field is expressed over.
    pub const DENOMINATOR: u32 = 1_000;
}

impl Atom for Hfov {
    const KIND: FourCC = FourCC::new(b"hfov");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        Ok(Hfov {
            horizontal_field_of_view: u32::decode(buf)?,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.horizontal_field_of_view.encode(buf)
    }
}
