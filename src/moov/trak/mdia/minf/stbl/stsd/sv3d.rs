use crate::*;

/// `sv3d` — the Spherical Video Box of **Google Spherical Video V2**.
///
/// A sibling of `colr` inside a visual sample entry, describing how the track's
/// picture maps onto a sphere:
///
/// ```text
/// sv3d  (Box)
///   svhd (FullBox v0)   metadata_source: null-terminated string
///   proj (Box)
///     prhd (FullBox v0) pose_yaw/pitch/roll_degrees: int32, 16.16 fixed
///     equi | cbmp | mshp
/// ```
///
/// The children are decoded positionally rather than through [`Any`] because
/// the Apple `vexu` box carries a **different** `proj` (see [`VexuProj`]) and a
/// FourCC names exactly one type in the [`Any`] dispatch.
///
/// Both children are optional even though the spec requires them. `sv3d` sits
/// inside a sample entry, so refusing one refuses the `stsd`, the `moov` and
/// therefore the whole file — over metadata a player can simply do without.
/// `libavformat` reads such a file: `mov_read_sv3d` `return 0`s (no spherical
/// metadata, no error) for a missing `svhd` and for a missing `proj`, and a
/// missing child is not worth a divergence between two readers of the same
/// file. Both types implement [`Default`], so a consumer that wants the
/// spec-implied value can say `unwrap_or_default()`; keeping the absence means
/// a re-encode does not invent a box the input did not have.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Sv3d {
    pub svhd: Option<Svhd>,
    pub proj: Option<Proj>,
}

impl Atom for Sv3d {
    const KIND: FourCC = FourCC::new(b"sv3d");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut svhd = None;
        let mut proj = None;

        while let Some(header) = Header::decode_maybe(buf)? {
            match header.kind {
                Svhd::KIND => {
                    if svhd.is_some() {
                        return Err(Error::DuplicateBox(Svhd::KIND));
                    }
                    svhd = Some(Svhd::decode_atom(&header, buf)?);
                }
                Proj::KIND => {
                    if proj.is_some() {
                        return Err(Error::DuplicateBox(Proj::KIND));
                    }
                    proj = Some(Proj::decode_atom(&header, buf)?);
                }
                _ => {
                    let unknown = Any::decode_atom(&header, buf)?;
                    Self::decode_unknown(&unknown)?;
                }
            }
        }

        skip_trailing_padding(buf);

        Ok(Sv3d { svhd, proj })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.svhd.encode(buf)?;
        self.proj.encode(buf)
    }
}

/// `svhd` — Spherical Video Header. Names the tool that wrote the metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Svhd {
    /// `metadata_source`: a null-terminated UTF-8 string naming the writer.
    pub metadata_source: String,
}

impl AtomExt for Svhd {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"svhd");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        Ok(Svhd {
            metadata_source: String::decode(buf)?,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        // No `Encode for String` exists — the wire form is the bytes plus the
        // NUL the decoder stops on, and `String` cannot contain an interior NUL
        // (`CString::new` in the decoder rejects one), so this round-trips.
        buf.append_slice(self.metadata_source.as_bytes());
        0u8.encode(buf)
    }
}

/// `proj` — the Projection Box of **Google Spherical Video V2**.
///
/// Not to be confused with [`VexuProj`], Apple's `proj` inside `vexu`: same
/// FourCC, different box.
///
/// Both children are optional, for the reason given on [`Sv3d`]: a missing
/// `prhd` is a `return 0` in `mov_read_sv3d`, and [`Prhd::default()`] — the
/// identity pose — is exactly what a missing pose means. A `kind` of `None`
/// says the box named no projection at all.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Proj {
    pub prhd: Option<Prhd>,
    pub kind: Option<ProjKind>,
}

impl Atom for Proj {
    const KIND: FourCC = FourCC::new(b"proj");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut prhd = None;
        let mut kind: Option<ProjKind> = None;

        // A helper so each projection-data arm reports the same duplicate error.
        macro_rules! set_kind {
            ($variant:ident, $ty:ident, $header:expr) => {{
                if kind.is_some() {
                    return Err(Error::DuplicateBox($ty::KIND));
                }
                kind = Some(ProjKind::$variant($ty::decode_atom($header, buf)?));
            }};
        }

        while let Some(header) = Header::decode_maybe(buf)? {
            match header.kind {
                Prhd::KIND => {
                    if prhd.is_some() {
                        return Err(Error::DuplicateBox(Prhd::KIND));
                    }
                    prhd = Some(Prhd::decode_atom(&header, buf)?);
                }
                Equi::KIND => set_kind!(Equi, Equi, &header),
                Cbmp::KIND => set_kind!(Cbmp, Cbmp, &header),
                Mshp::KIND => set_kind!(Mshp, Mshp, &header),
                // Anything else here IS the projection data box: `proj` holds
                // `prhd` plus exactly one of them, and the box is explicitly
                // extensible, so a FourCC this crate does not model is a
                // projection from a later revision rather than a broken file.
                // Kept verbatim so a re-encode does not drop it.
                _ => {
                    if kind.is_some() {
                        return Err(Error::DuplicateBox(header.kind));
                    }
                    let size = header.size.unwrap_or(buf.remaining());
                    if size > buf.remaining() {
                        return Err(Error::OutOfBounds);
                    }
                    let data = buf.slice(size).to_vec();
                    buf.advance(size);
                    kind = Some(ProjKind::Unknown(header.kind, data));
                }
            }
        }
        skip_trailing_padding(buf);

        Ok(Proj { prhd, kind })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.prhd.encode(buf)?;
        match &self.kind {
            Some(ProjKind::Equi(atom)) => atom.encode(buf),
            Some(ProjKind::Cbmp(atom)) => atom.encode(buf),
            Some(ProjKind::Mshp(atom)) => atom.encode(buf),
            Some(ProjKind::Unknown(kind, data)) => {
                Header {
                    kind: *kind,
                    size: Some(data.len()),
                }
                .encode(buf)?;
                buf.append_slice(data);
                Ok(())
            }
            None => Ok(()),
        }
    }
}

/// `prhd` — Projection Header. A rotation applied to the sphere's CONTENT,
/// order yaw → pitch → roll, each an int32 in **16.16 fixed-point degrees**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Prhd {
    pub pose_yaw_degrees: i32,
    pub pose_pitch_degrees: i32,
    pub pose_roll_degrees: i32,
}

impl AtomExt for Prhd {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"prhd");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        Ok(Prhd {
            pose_yaw_degrees: i32::decode(buf)?,
            pose_pitch_degrees: i32::decode(buf)?,
            pose_roll_degrees: i32::decode(buf)?,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.pose_yaw_degrees.encode(buf)?;
        self.pose_pitch_degrees.encode(buf)?;
        self.pose_roll_degrees.encode(buf)
    }
}

/// The one projection-data box a [`Proj`] carries.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum ProjKind {
    Equi(Equi),
    Cbmp(Cbmp),
    Mshp(Mshp),
    /// A projection this crate does not model, kept as its FourCC and the body
    /// bytes after the 8-byte box header. Google Spherical Video V2 says the
    /// projection box set is extensible, so this is a valid file, not a broken
    /// one — a consumer that cannot render the projection still round-trips it.
    Unknown(FourCC, Vec<u8>),
}

/// `equi` — Equirectangular Projection. Each bound is a **0.32 fixed-point**
/// fraction of the full sphere cropped away on that side; all-zero = the full
/// sphere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Equi {
    pub projection_bounds_top: u32,
    pub projection_bounds_bottom: u32,
    pub projection_bounds_left: u32,
    pub projection_bounds_right: u32,
}

impl AtomExt for Equi {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"equi");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        // Wire order is top, bottom, left, right (`libavformat/mov.c`
        // `mov_read_sv3d`), NOT the `AVSphericalMapping` field order.
        Ok(Equi {
            projection_bounds_top: u32::decode(buf)?,
            projection_bounds_bottom: u32::decode(buf)?,
            projection_bounds_left: u32::decode(buf)?,
            projection_bounds_right: u32::decode(buf)?,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.projection_bounds_top.encode(buf)?;
        self.projection_bounds_bottom.encode(buf)?;
        self.projection_bounds_left.encode(buf)?;
        self.projection_bounds_right.encode(buf)
    }
}

/// `cbmp` — Cubemap Projection. `layout` 0 is the spec's 3×2 grid; 1..=255 are
/// reserved and are carried verbatim rather than guessed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Cbmp {
    pub layout: u32,
    /// Pixels of padding around each cube face.
    pub padding: u32,
}

impl AtomExt for Cbmp {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"cbmp");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        Ok(Cbmp {
            layout: u32::decode(buf)?,
            padding: u32::decode(buf)?,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.layout.encode(buf)?;
        self.padding.encode(buf)
    }
}

/// `mshp` — Mesh Projection. The mesh payload is retained verbatim: a consumer
/// that cannot render a mesh still has everything it needs to later, without
/// re-opening the container.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mshp {
    pub crc: u32,
    /// `'raw '` (uncompressed) or `'dfl8'` (raw deflate).
    pub encoding_four_cc: FourCC,
    /// Everything after the header — one or more `mesh` boxes, deflated when
    /// `encoding_four_cc` says so.
    pub data: Vec<u8>,
}

impl AtomExt for Mshp {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"mshp");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        Ok(Mshp {
            crc: u32::decode(buf)?,
            encoding_four_cc: FourCC::decode(buf)?,
            data: Vec::decode(buf)?,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.crc.encode(buf)?;
        self.encoding_four_cc.encode(buf)?;
        self.data.encode(buf)
    }
}
