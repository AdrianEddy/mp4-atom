use crate::{Any, Buf, FourCC, MAX_ATOM_DEPTH};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("out of bounds")]
    OutOfBounds,

    #[error("short read")]
    ShortRead,

    #[error("over decode: {0}")]
    OverDecode(FourCC),

    #[error("under decode: {0}")]
    UnderDecode(FourCC),

    #[error("atom too large")]
    TooLarge(FourCC),

    #[error("atoms nested more than {MAX_ATOM_DEPTH} deep: {0}")]
    TooDeep(FourCC),

    #[error("invalid size")]
    InvalidSize,

    #[error("invalid fourcc")]
    InvalidFourCC,

    #[error("unknown version: {0}")]
    UnknownVersion(u8),

    #[error("invalid string: {0}")]
    InvalidString(String),

    #[error("missing box: {0}")]
    MissingBox(FourCC),

    #[error("unexpected box: {0}")]
    UnexpectedBox(FourCC),

    #[error("duplicate box: {0}")]
    DuplicateBox(FourCC),

    #[error("missing descriptor: {0}")]
    MissingDescriptor(u8),

    #[error("unexpected descriptor: {0}")]
    UnexpectedDescriptor(u8),

    #[error("unexpected eof")]
    UnexpectedEof,

    #[error("unknown quicktime version: {0}")]
    UnknownQuicktimeVersion(u16),

    #[error("unsupported: {0}")]
    Unsupported(&'static str),

    // Returned in the rare case when we can't represent a value in the desired type
    #[error("out of memory")]
    OutOfMemory,

    #[error("reserved")]
    Reserved,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("missing required content: {0}")]
    MissingContent(&'static str),

    #[error("invalid parameter combination: {0}")]
    InvalidCombination(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Either logs or returns an error depending on the strict feature flag.
pub(crate) fn decode_unknown(atom: &Any, parent: FourCC) -> Result<()> {
    if cfg!(feature = "strict") || cfg!(test) {
        tracing::error!(kind = %atom.kind(), parent = %parent, "unexpected box");
        return Err(Error::UnexpectedBox(atom.kind()));
    } else {
        tracing::warn!(kind = %atom.kind(), parent = %parent, "unexpected box");
    }
    Ok(())
}

/// Skip a child of `size` bytes that does not decode, or a second copy of one
/// that `parent` holds at most once, as an unknown child is skipped: the boxes
/// that carry tags (`meta`, `udta` and the typed `udta` children) were unknown
/// children before this crate typed them, and one malformed tag must not make
/// the movie unreadable. libavformat reads past every one of them. `buf` stands
/// at the child's body. Under `strict` (and in this crate's tests) the error
/// stands.
pub(crate) fn skip_malformed<B: Buf>(
    kind: FourCC,
    parent: FourCC,
    size: usize,
    buf: &mut B,
    err: Error,
) -> Result<()> {
    if cfg!(feature = "strict") || cfg!(test) {
        tracing::error!(%kind, %parent, %err, "malformed box");
        return Err(err);
    }
    tracing::warn!(%kind, %parent, %err, "malformed box skipped");
    buf.advance(size);
    Ok(())
}
