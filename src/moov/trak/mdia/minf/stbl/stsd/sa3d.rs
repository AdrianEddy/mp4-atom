use crate::*;

/// `SA3D` — the Spatial Audio Box of Google's **Spatial Audio RFC**
/// (`google/spatial-media`, `docs/spatial-audio-rfc.md`).
///
/// A child of a sound sample entry (`mp4a`, `Opus`, `fLaC`, `lpcm`, `sowt`, …)
/// that makes the track's channels an ambisonic sound field, optionally
/// followed by a head-locked stereo pair that does not turn with the scene.
///
/// Version 0 is the only one the RFC defines, and its layout is the one read
/// and written here. Every field is carried as the spec's own value rather than
/// an enum, as [`St3d`] carries its `stereo_mode`: the RFC defines one value
/// for each of `ambisonic_type` ([`Self::PERIPHONIC`]),
/// `ambisonic_channel_ordering` ([`Self::ACN`]) and `ambisonic_normalization`
/// ([`Self::SN3D`]), and a reader checks them (and `version`) before it takes
/// the channels as a field. Keeping the values means anything else survives a
/// decode → encode round trip.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Sa3d {
    /// `0` for the layout this box models.
    pub version: u8,
    /// `head_locked_stereo`: the track carries a head-locked stereo pair after
    /// its ambisonic components (the top bit of the RFC's `ambisonic_type`
    /// byte).
    pub head_locked_stereo: bool,
    /// `ambisonic_type`, the low seven bits of its byte.
    pub ambisonic_type: u8,
    /// `ambisonic_order`: a periphonic field of order `n` has `(n + 1)²`
    /// components.
    pub ambisonic_order: u32,
    /// `ambisonic_channel_ordering`.
    pub ambisonic_channel_ordering: u8,
    /// `ambisonic_normalization`.
    pub ambisonic_normalization: u8,
    /// `channel_map`, one entry per channel of the track (`num_channels` is its
    /// length): the ambisonic component (in the box's channel ordering) the
    /// channel holds, the head-locked pair numbered after the components. The
    /// identity map means the channels are stored in that ordering.
    pub channel_map: Vec<u32>,
}

impl Sa3d {
    /// `ambisonic_type` 0 — a periphonic (full-sphere) sound field.
    pub const PERIPHONIC: u8 = 0;
    /// `ambisonic_channel_ordering` 0 — Ambisonic Channel Number order.
    pub const ACN: u8 = 0;
    /// `ambisonic_normalization` 0 — Schmidt semi-normalisation.
    pub const SN3D: u8 = 0;
}

impl Atom for Sa3d {
    const KIND: FourCC = FourCC::new(b"SA3D");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let version = u8::decode(buf)?;
        let kind = u8::decode(buf)?;
        let ambisonic_order = u32::decode(buf)?;
        let ambisonic_channel_ordering = u8::decode(buf)?;
        let ambisonic_normalization = u8::decode(buf)?;
        let num_channels = u32::decode(buf)? as usize;
        // Bounded by the bytes the box holds, so a hostile count cannot size
        // the allocation.
        if num_channels > buf.remaining() / 4 {
            return Err(Error::OutOfBounds);
        }
        let channel_map = (0..num_channels)
            .map(|_| u32::decode(buf))
            .collect::<Result<_>>()?;
        Ok(Sa3d {
            version,
            head_locked_stereo: kind & 0x80 != 0,
            ambisonic_type: kind & 0x7f,
            ambisonic_order,
            ambisonic_channel_ordering,
            ambisonic_normalization,
            channel_map,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        if self.ambisonic_type > 0x7f {
            return Err(Error::InvalidCombination(
                "ambisonic_type has seven bits; the eighth is head_locked_stereo",
            ));
        }
        let num_channels =
            u32::try_from(self.channel_map.len()).map_err(|_| Error::TooLarge(Self::KIND))?;
        self.version.encode(buf)?;
        (u8::from(self.head_locked_stereo) << 7 | self.ambisonic_type).encode(buf)?;
        self.ambisonic_order.encode(buf)?;
        self.ambisonic_channel_ordering.encode(buf)?;
        self.ambisonic_normalization.encode(buf)?;
        num_channels.encode(buf)?;
        for channel in &self.channel_map {
            channel.encode(buf)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The RFC's own example: a first-order field stored W X Y Z, mapped to
    /// ACN as `0 2 3 1`.
    const RFC_EXAMPLE: &[u8] = &[
        0x00, 0x00, 0x00, 0x24, b'S', b'A', b'3', b'D', // header
        0x00, // version
        0x00, // head_locked_stereo, ambisonic_type
        0x00, 0x00, 0x00, 0x01, // ambisonic_order
        0x00, // ambisonic_channel_ordering
        0x00, // ambisonic_normalization
        0x00, 0x00, 0x00, 0x04, // num_channels
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
        0x01,
    ];

    #[test]
    fn test_sa3d_rfc_example() {
        let decoded = Sa3d::decode(&mut &RFC_EXAMPLE[..]).unwrap();
        let expected = Sa3d {
            version: 0,
            head_locked_stereo: false,
            ambisonic_type: Sa3d::PERIPHONIC,
            ambisonic_order: 1,
            ambisonic_channel_ordering: Sa3d::ACN,
            ambisonic_normalization: Sa3d::SN3D,
            channel_map: vec![0, 2, 3, 1],
        };
        assert_eq!(decoded, expected);

        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();
        assert_eq!(buf, RFC_EXAMPLE);
    }

    /// The head-locked pair rides the top bit of the type byte, and numbers
    /// after the components in the map, as libavformat's `mov_write_SA3D_tag`
    /// writes it.
    #[test]
    fn test_sa3d_head_locked_stereo() {
        let sa3d = Sa3d {
            head_locked_stereo: true,
            ambisonic_order: 1,
            channel_map: (0..6).collect(),
            ..Default::default()
        };
        let mut buf = Vec::new();
        sa3d.encode(&mut buf).unwrap();
        assert_eq!(&buf[8..10], &[0x00, 0x80]);
        assert_eq!(buf.len(), 8 + 12 + 6 * 4);
        assert_eq!(Sa3d::decode(&mut &buf[..]).unwrap(), sa3d);
    }

    /// A type that does not fit its seven bits is refused, not folded into
    /// the head-locked flag.
    #[test]
    fn test_sa3d_type_out_of_range() {
        let sa3d = Sa3d {
            ambisonic_type: 0x80,
            ..Default::default()
        };
        assert!(sa3d.encode(&mut Vec::new()).is_err());
    }

    /// A channel count the box has no room for fails the decode instead of
    /// sizing an allocation.
    #[test]
    fn test_sa3d_count_past_the_box() {
        let mut bytes = RFC_EXAMPLE.to_vec();
        bytes[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(Sa3d::decode(&mut &bytes[..]).is_err());
    }
}
