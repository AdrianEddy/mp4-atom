mod edts;
mod mdia;
mod senc;
mod tkhd;
mod tref;

pub use edts::*;
pub use mdia::*;
pub use senc::*;
pub use tkhd::*;
pub use tref::*;

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Trak {
    pub tkhd: Tkhd,
    pub edts: Option<Edts>,
    pub meta: Option<Meta>, // TODO is this suppose to be here?
    pub mdia: Mdia,
    pub senc: Option<Senc>,
    pub tref: Option<Tref>,
    pub udta: Option<Udta>,
    /// Extended-type boxes carried directly by this track, in file order.
    /// Spherical Video V1 rides one of these
    /// (`ffcc8263-f855-4a93-8814-587a02521fdd`).
    pub uuid: Vec<Uuid>,
}

impl Atom for Trak {
    const KIND: FourCC = FourCC::new(b"trak");

    nested! {
        required: [ Tkhd, Mdia ],
        optional: [ Edts, Meta, Senc, Tref, Udta ],
        multiple: [ Uuid ],
    }
}
