use std::io::{Cursor, Read};

use crate::*;

/// A atom header, which contains the atom's kind and size.
#[derive(Debug, Clone, Copy)]
pub struct Header {
    /// The name of the atom, always 4 bytes.
    pub kind: FourCC,

    /// The size of the atom, **excluding** the header.
    /// This is optional when the atom extends to the end of the file.
    ///
    /// A `u64`, as the header states it: a box can be larger than a 32-bit
    /// target's `usize`, and a reader walking a file must still skip it.
    /// [`Header::size_within`] gives the size as an in-memory length.
    pub size: Option<u64>,
}

impl Encode for Header {
    fn encode<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        match self.size {
            Some(size) if size > u64::from(u32::MAX) - 8 => {
                // Have to include the size of this extra field
                let total = size.checked_add(16).ok_or(Error::TooLarge(self.kind))?;
                1u32.encode(buf)?;
                self.kind.encode(buf)?;
                total.encode(buf)
            }
            Some(size) => {
                (size as u32 + 8).encode(buf)?;
                self.kind.encode(buf)
            }
            None => {
                0u32.encode(buf)?;
                self.kind.encode(buf)
            }
        }
    }
}

impl Decode for Header {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self> {
        let size = u32::decode(buf)?;
        let kind = FourCC::decode(buf)?;

        let size = match size {
            0 => None,
            1 => {
                // Read another 8 bytes
                let size = u64::decode(buf)?;
                Some(size.checked_sub(16).ok_or(Error::InvalidSize)?)
            }
            _ => Some(u64::from(size.checked_sub(8).ok_or(Error::InvalidSize)?)),
        };

        Ok(Self { kind, size })
    }
}

impl DecodeMaybe for Header {
    fn decode_maybe<B: Buf>(buf: &mut B) -> Result<Option<Self>> {
        if buf.remaining() < 8 {
            return Ok(None);
        }

        let size = u32::from_be_bytes(buf.slice(4).try_into().unwrap());
        if size == 1 && buf.remaining() < 16 {
            return Ok(None);
        }

        Ok(Some(Self::decode(buf)?))
    }
}

impl ReadFrom for Header {
    fn read_from<R: Read + ?Sized>(r: &mut R) -> Result<Self> {
        <Option<Header> as ReadFrom>::read_from(r)?.ok_or(Error::UnexpectedEof)
    }
}

impl ReadFrom for Option<Header> {
    fn read_from<R: Read + ?Sized>(r: &mut R) -> Result<Self> {
        let mut buf = [0u8; 8];
        let n = r.read(&mut buf)?;
        if n == 0 {
            return Ok(None);
        }

        r.read_exact(&mut buf[n..])?;

        let size = u32::from_be_bytes(buf[0..4].try_into().unwrap());
        let kind = u32::from_be_bytes(buf[4..8].try_into().unwrap()).into();

        let size = match size {
            0 => None,
            1 => {
                // Read another 8 bytes
                r.read_exact(&mut buf)?;
                let size = u64::from_be_bytes(buf);
                Some(size.checked_sub(16).ok_or(Error::InvalidSize)?)
            }
            _ => Some(u64::from(size.checked_sub(8).ok_or(Error::InvalidSize)?)),
        };

        Ok(Some(Header { kind, size }))
    }
}

// Utility methods
impl Header {
    /// The body's length in a buffer holding `remaining` bytes after the
    /// header: the stated size, or all of `remaining` for an atom that extends
    /// to the end. `None` when the stated size is more than `remaining`, which
    /// includes any size the target's `usize` cannot hold.
    pub fn size_within(&self, remaining: usize) -> Option<usize> {
        match self.size {
            None => Some(remaining),
            Some(size) => usize::try_from(size).ok().filter(|&size| size <= remaining),
        }
    }

    /// The stated size as an in-memory length, or [`Error::TooLarge`] when the
    /// target's `usize` cannot hold it.
    fn memory_size(&self) -> Result<Option<usize>> {
        self.size
            .map(|size| usize::try_from(size).map_err(|_| Error::TooLarge(self.kind)))
            .transpose()
    }

    pub(crate) fn read_body<R: Read + ?Sized>(&self, r: &mut R) -> Result<Cursor<Vec<u8>>> {
        // TODO This allocates on the heap.
        // Ideally, we should use ReadFrom instead of Decode to avoid this.

        // Don't use `with_capacity` on an untrusted size
        // We allocate at most 4096 bytes upfront and grow as needed
        // A body is read into memory, so it must fit in it.
        let size = self.memory_size()?;
        let cap = size.unwrap_or(0).min(4096);
        let mut buf = Vec::with_capacity(cap);

        match self.size {
            Some(size) => {
                let n = std::io::copy(&mut r.take(size), &mut buf)?;
                if size != n {
                    return Err(Error::OutOfBounds);
                }
            }
            None => {
                std::io::copy(r, &mut buf)?;
            }
        };

        Ok(Cursor::new(buf))
    }

    #[cfg(feature = "tokio")]
    pub(crate) async fn read_body_tokio<R: ::tokio::io::AsyncRead + Unpin + ?Sized>(
        &self,
        r: &mut R,
    ) -> Result<Cursor<Vec<u8>>> {
        use ::tokio::io::AsyncReadExt;

        // TODO This allocates on the heap.
        // Ideally, we should use ReadFrom instead of Decode to avoid this.

        // Don't use `with_capacity` on an untrusted size
        // We allocate at most 4096 bytes upfront and grow as needed
        // A body is read into memory, so it must fit in it.
        let size = self.memory_size()?;
        let cap = size.unwrap_or(0).min(4096);
        let mut buf = Vec::with_capacity(cap);

        match self.size {
            Some(size) => {
                let n = ::tokio::io::copy(&mut r.take(size), &mut buf).await?;
                if size != n {
                    return Err(Error::OutOfBounds);
                }
            }
            None => {
                ::tokio::io::copy(r, &mut buf).await?;
            }
        };

        Ok(Cursor::new(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A 5 GiB `mdat` header: `size` 1, then the 64-bit `largesize`. Header
    // only, no payload.
    const LARGE_SIZE: u64 = 5 << 30;
    const LARGE_HEADER: [u8; 16] = [
        0, 0, 0, 1, b'm', b'd', b'a', b't', 0, 0, 0, 1, 0x40, 0, 0, 0,
    ];

    // A `largesize` above `u32::MAX` is kept whole, by every reader: as a
    // `usize` a 32-bit target truncated it (5 GiB read as 1 GiB), and a walk
    // that skipped the box by it landed inside the box.
    #[test]
    fn test_header_largesize_above_u32() {
        assert_eq!(
            u64::from_be_bytes(LARGE_HEADER[8..].try_into().unwrap()),
            LARGE_SIZE
        );

        let decoded = Header::decode(&mut &LARGE_HEADER[..]).unwrap();
        assert_eq!(decoded.kind, FourCC::new(b"mdat"));
        assert_eq!(decoded.size, Some(LARGE_SIZE - 16));

        let read = <Header as ReadFrom>::read_from(&mut &LARGE_HEADER[..]).unwrap();
        assert_eq!(read.size, Some(LARGE_SIZE - 16));

        let mut encoded = Vec::new();
        decoded.encode(&mut encoded).unwrap();
        assert_eq!(encoded, LARGE_HEADER);
    }

    // A body read into memory must fit there: no buffer holds a body over
    // `usize::MAX`, nor one larger than what remains.
    #[test]
    fn test_header_size_within() {
        let header = |size| Header {
            kind: FourCC::new(b"free"),
            size,
        };
        assert_eq!(header(Some(8)).size_within(8), Some(8));
        assert_eq!(header(Some(9)).size_within(8), None);
        assert_eq!(header(None).size_within(8), Some(8));
        assert_eq!(
            header(Some(LARGE_SIZE)).size_within(usize::MAX),
            usize::try_from(LARGE_SIZE).ok()
        );
        assert_eq!(
            header(Some(u64::MAX)).size_within(usize::MAX),
            usize::try_from(u64::MAX).ok()
        );
    }

    // The compact form up to `u32::MAX` including the header, the `largesize`
    // form above it, and no wrap at the top of the range.
    #[test]
    fn test_header_encode_size_forms() {
        let encode = |size| {
            let mut buf = Vec::new();
            Header {
                kind: FourCC::new(b"mdat"),
                size: Some(size),
            }
            .encode(&mut buf)
            .map(|()| buf)
        };
        assert_eq!(
            encode(u64::from(u32::MAX) - 8).unwrap()[..4],
            u32::MAX.to_be_bytes()
        );
        let large = encode(u64::from(u32::MAX) - 7).unwrap();
        assert_eq!(large[..4], 1u32.to_be_bytes());
        assert_eq!(large[8..], (u64::from(u32::MAX) + 9).to_be_bytes());
        // Too large for the 64-bit field: an error, and nothing written.
        let mut buf = Vec::new();
        let header = Header {
            kind: FourCC::new(b"mdat"),
            size: Some(u64::MAX - 15),
        };
        assert!(matches!(header.encode(&mut buf), Err(Error::TooLarge(_))));
        assert!(buf.is_empty());
    }
}
