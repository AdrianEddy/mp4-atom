mod dinf;
mod gmhd;
mod hmhd;
mod nmhd;
mod smhd;
mod stbl;
mod sthd;
mod vmhd;

pub use dinf::*;
pub use gmhd::*;
pub use hmhd::*;
pub use nmhd::*;
pub use smhd::*;
pub use stbl::*;
pub use sthd::*;
pub use vmhd::*;

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Minf {
    pub vmhd: Option<Vmhd>,
    pub smhd: Option<Smhd>,
    pub nmhd: Option<Nmhd>,
    pub sthd: Option<Sthd>,
    pub hmhd: Option<Hmhd>,
    /// QuickTime base media information header — a timecode / text track's.
    pub gmhd: Option<Gmhd>,
    /// QuickTime's data handler reference (`hdlr` with component type `dhlr`),
    /// which QuickTime writers put in `minf`. ISO files have none.
    pub hdlr: Option<Hdlr>,
    pub dinf: Dinf,
    pub stbl: Stbl,
}

impl Atom for Minf {
    const KIND: FourCC = FourCC::new(b"minf");

    // The media header first, then QuickTime's data handler, `dinf` and
    // `stbl` — the order QuickTime readers expect (QTFF) and ISO/IEC 14496-12
    // lists.
    nested! {
        required: [ Dinf, Stbl ],
        optional: [ Vmhd, Smhd, Nmhd, Sthd, Hmhd, Gmhd, Hdlr ],
        multiple: [],
        order: [ Vmhd, Smhd, Nmhd, Sthd, Hmhd, Gmhd, Hdlr, Dinf, Stbl ],
    }
}
