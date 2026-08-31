use crate::*;

/// `st3d` — the Stereoscopic 3D Video Box of **Google Spherical Video V2**.
///
/// A sibling of `sv3d` inside a visual sample entry, so it describes a
/// rectilinear track exactly as it describes a spherical one.
///
/// `stereo_mode` is carried as the spec's own byte rather than an enum: the
/// four defined values are `0` mono, `1` top-bottom (left eye on top), `2`
/// left-right (left eye on the left) and `3` stereo-custom (mesh only), and
/// every other value is reserved. Keeping the byte means a reserved value
/// survives a decode → encode round trip instead of being collapsed into a
/// spelling this crate invented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct St3d {
    pub stereo_mode: u8,
}

impl St3d {
    /// `stereo_mode` 0 — one view, nothing packed.
    pub const MONO: u8 = 0;
    /// `stereo_mode` 1 — top-bottom, left eye on top.
    pub const TOP_BOTTOM: u8 = 1;
    /// `stereo_mode` 2 — left-right, left eye on the left.
    pub const LEFT_RIGHT: u8 = 2;
    /// `stereo_mode` 3 — stereo-custom; the packing is described by an `mshp`
    /// mesh and has no rectangular crop.
    pub const STEREO_CUSTOM: u8 = 3;
}

impl AtomExt for St3d {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"st3d");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        Ok(St3d {
            stereo_mode: u8::decode(buf)?,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.stereo_mode.encode(buf)
    }
}
