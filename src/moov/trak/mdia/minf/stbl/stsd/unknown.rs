use crate::*;

/// The codec-independent view of a sample entry whose four-character code this
/// crate does not model.
///
/// [`Codec::Unknown`] keeps such an entry's body verbatim — everything after
/// the 8-byte box header. An unmodelled *visual* entry (CineForm `CFHD`,
/// ProRes, DV, …) still carries child boxes worth reading, `colr` / `sv3d` /
/// `st3d` / `vexu` / `hfov` among them, whose meaning does not depend on the
/// codec. Decoding those bytes as an `UnknownVisual` reads them without this
/// crate having to model the codec itself:
///
/// ```no_run
/// # use mp4_atom::{Codec, Decode, UnknownVisual};
/// # fn example(codec: &Codec) -> mp4_atom::Result<()> {
/// if let Codec::Unknown(kind, body) = codec {
///     let visual = UnknownVisual::decode(&mut body.as_slice())?;
///     println!("{kind} is {}x{}", visual.visual.width, visual.visual.height);
/// }
/// # Ok(())
/// # }
/// ```
///
/// **Only meaningful when the containing track's handler is `vide`.** A sample
/// entry carries no discriminator between the visual and audio shapes — they
/// share the 8-byte `SampleEntry` prefix and diverge after it — so the caller,
/// which knows the handler type, states which one this is. Reading an audio
/// entry this way yields garbage or an error.
///
/// The boxes named below are the ones whose meaning does not depend on the
/// codec; everything else is kept in [`UnknownVisual::other`], in file order,
/// so nothing is lost.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UnknownVisual {
    pub visual: Visual,
    /// Google Spherical Video V2.
    pub sv3d: Option<Sv3d>,
    /// Google Spherical Video V2 stereo mode.
    pub st3d: Option<St3d>,
    /// Apple Projected Media Profile.
    pub vexu: Option<Vexu>,
    /// Apple horizontal field of view.
    pub hfov: Option<Hfov>,
    /// Every other child box, in file order. Unmodelled ones keep their bytes
    /// as [`Any::Unknown`].
    pub other: Vec<Any>,
}

/// Errors when the body is shorter than the visual header, or when a child box
/// is truncated.
impl Decode for UnknownVisual {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self> {
        let visual = Visual::decode(buf)?;

        let mut out = UnknownVisual {
            visual,
            ..Default::default()
        };

        while let Some(header) = Header::decode_maybe(buf)? {
            match header.kind {
                Sv3d::KIND => {
                    if out.sv3d.is_some() {
                        return Err(Error::DuplicateBox(Sv3d::KIND));
                    }
                    out.sv3d = Some(Sv3d::decode_atom(&header, buf)?);
                }
                St3d::KIND => {
                    if out.st3d.is_some() {
                        return Err(Error::DuplicateBox(St3d::KIND));
                    }
                    out.st3d = Some(St3d::decode_atom(&header, buf)?);
                }
                Vexu::KIND => {
                    if out.vexu.is_some() {
                        return Err(Error::DuplicateBox(Vexu::KIND));
                    }
                    out.vexu = Some(Vexu::decode_atom(&header, buf)?);
                }
                Hfov::KIND => {
                    if out.hfov.is_some() {
                        return Err(Error::DuplicateBox(Hfov::KIND));
                    }
                    out.hfov = Some(Hfov::decode_atom(&header, buf)?);
                }
                _ => out.other.push(Any::decode_atom(&header, buf)?),
            }
        }
        skip_trailing_padding(buf);

        Ok(out)
    }
}
