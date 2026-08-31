use std::cell::Cell;

use crate::{Error, FourCC, Result};

/// The longest chain of nested atoms [`Any`](crate::Any) will descend through
/// before refusing with [`Error::TooDeep`].
///
/// A box's payload is more boxes, so decoding is recursive and the recursion is
/// driven entirely by the file: nesting one container FourCC inside itself a
/// few hundred times is a few kilobytes of input and an unbounded descent,
/// which overflows the stack rather than returning an error. Every such cycle
/// passes through [`Any::decode_atom`](crate::DecodeAtom::decode_atom), which
/// is where the limit is enforced.
///
/// The value is a property of the format, not a tuning knob. ISO/IEC 14496-12's
/// hierarchy is finite and shallow: the deepest chain this crate models is
/// `moov` / `trak` / `mdia` / `minf` / `stbl` / `stsd` / *sample entry* /
/// *codec configuration* — eight levels — and a container whose FourCC is not
/// modelled is kept verbatim as [`Any::Unknown`](crate::Any::Unknown) rather
/// than descended into, so unknown boxes cost no depth at all. Twelve leaves
/// room for the hierarchy to grow without letting a few hundred bytes of input
/// consume the stack. (`libavformat` caps the same recursion at ten, in
/// `mov_read_default`.)
pub const MAX_ATOM_DEPTH: u32 = 12;

thread_local! {
    /// How many nested atoms this thread is currently decoding.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Counts one level of atom nesting, releasing it when dropped.
///
/// Decoding is a recursive descent over a `&mut impl Buf` with no context
/// object to carry a counter on — [`Decode`](crate::Decode) and
/// [`Atom`](crate::Atom) take the buffer and nothing else — and the buffer a
/// child is decoded from is a fresh sub-slice, so the depth cannot ride along
/// with it either. A thread-local counter is therefore where the state lives;
/// it is scoped to a single synchronous decode, and the guard's `Drop` releases
/// it on every path, including the `?` returns and an unwinding panic.
pub(crate) struct DepthGuard;

impl DepthGuard {
    /// Enter `kind`, or fail with [`Error::TooDeep`] if that would exceed
    /// [`MAX_ATOM_DEPTH`].
    pub(crate) fn enter(kind: FourCC) -> Result<Self> {
        DEPTH.with(|depth| {
            let entered = depth.get() + 1;
            if entered > MAX_ATOM_DEPTH {
                return Err(Error::TooDeep(kind));
            }
            depth.set(entered);
            Ok(DepthGuard)
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}
