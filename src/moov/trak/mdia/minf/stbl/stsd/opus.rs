use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Opus {
    pub audio: Audio,
    pub dops: Dops,
    /// The spatial-audio box: the channels are an ambisonic sound field.
    pub sa3d: Option<Sa3d>,
    pub btrt: Option<Btrt>,
}

impl Atom for Opus {
    const KIND: FourCC = FourCC::new(b"Opus");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let audio = Audio::decode(buf)?;

        let mut dops = None;
        let mut sa3d = None;
        let mut btrt = None;

        // Find d0ps in mp4a or wave
        while let Some(atom) = Any::decode_maybe(buf)? {
            match atom {
                Any::Dops(atom) => dops = atom.into(),
                Any::Sa3d(atom) => sa3d = atom.into(),
                Any::Btrt(atom) => btrt = atom.into(),
                unknown => Self::decode_unknown(&unknown)?,
            }
        }
        skip_trailing_padding(buf);

        Ok(Self {
            audio,
            dops: dops.ok_or(Error::MissingBox(Dops::KIND))?,
            sa3d,
            btrt,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.audio.encode(buf)?;
        self.dops.encode(buf)?;
        self.sa3d.encode(buf)?;
        self.btrt.encode(buf)?;
        Ok(())
    }
}

/*
    class ChannelMappingTable (unsigned int(8) OutputChannelCount){
        unsigned int(8) StreamCount;
        unsigned int(8) CoupledCount;
        unsigned int(8 * OutputChannelCount) ChannelMapping;
    }

    aligned(8) class OpusSpecificBox extends Box('dOps'){
        unsigned int(8) Version;
        unsigned int(8) OutputChannelCount;
        unsigned int(16) PreSkip;
        unsigned int(32) InputSampleRate;
        signed int(16) OutputGain;
        unsigned int(8) ChannelMappingFamily;
        if (ChannelMappingFamily != 0) {
            ChannelMappingTable(OutputChannelCount);
        }
    }

    The ChannelMapping field is "the same octet string as the Channel Mapping
    field of the Ogg Opus identification header". For channel mapping family 3
    (RFC 8486 §3.2, later than the encapsulation) that header's table holds a
    demixing matrix in place of the per-channel indices, so the octet string is
    the matrix: 2 × (StreamCount + CoupledCount) × OutputChannelCount octets.
*/

/// The Opus Specific Box (`dOps`) of *Encapsulation of Opus in ISO Base Media
/// File Format* (version 1.0.0): the Ogg Opus identification header (RFC 7845
/// §5.1) less its magic and version, its fields big-endian.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Dops {
    pub output_channel_count: u8,
    pub pre_skip: u16,
    pub input_sample_rate: u32,
    pub output_gain: i16,
    /// The channel mapping family and its table, or `None` for family 0 (mono
    /// or stereo, no table).
    pub channel_mapping: Option<OpusChannelMapping>,
}

/// A nonzero channel mapping family and its table, laid out as RFC 7845
/// §5.1.1 describes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OpusChannelMapping {
    /// The channel mapping family; never 0, which has no table.
    pub family: u8,
    /// The number of Opus streams in each packet.
    pub stream_count: u8,
    /// How many of those streams are coupled (stereo); they come first.
    pub coupled_count: u8,
    /// The rest of the table, as the Ogg identification header holds it.
    ///
    /// In every family but [`Self::DEMIXING_MATRIX`], one entry per output
    /// channel: the decoded channel feeding it, or 255 for silence.
    ///
    /// In family [`Self::DEMIXING_MATRIX`] (RFC 8486 §3.2), the demixing
    /// matrix: one row per output channel and one column per decoded channel
    /// (`stream_count + coupled_count`), stored column by column, each
    /// coefficient a little-endian signed Q15 in two octets, as the Ogg header
    /// stores it.
    pub channel_mapping: Vec<u8>,
}

impl OpusChannelMapping {
    /// Family 3 (RFC 8486 §3.2): ambisonics coded through a demixing matrix,
    /// which the table holds in place of the per-channel indices.
    pub const DEMIXING_MATRIX: u8 = 3;

    /// The decoded channel count, `stream_count + coupled_count`.
    fn decoded_channels(&self) -> u16 {
        u16::from(self.stream_count) + u16::from(self.coupled_count)
    }

    /// The octets the table holds after its two counts for
    /// `output_channel_count` output channels.
    fn table_len(&self, output_channel_count: u8) -> usize {
        let outputs = usize::from(output_channel_count);
        if self.family == Self::DEMIXING_MATRIX {
            2 * usize::from(self.decoded_channels()) * outputs
        } else {
            outputs
        }
    }

    // The checks RFC 7845 §5.1.1 applies to every family, so a table read from
    // a file is refused rather than trusted. Family-specific channel counts
    // (family 1 up to 8, families 2 and 3 a complete ambisonic order) are the
    // consumer's to check.
    fn validate(&self, output_channel_count: u8) -> Result<()> {
        self.validate_counts(output_channel_count)?;
        self.validate_table(output_channel_count)
    }

    // The checks on the counts alone. They bound the table to
    // 2 × 255 × 255 octets, so decode runs them before reading it.
    fn validate_counts(&self, output_channel_count: u8) -> Result<()> {
        if self.family == 0 {
            return Err(Error::InvalidCombination(
                "Opus channel mapping family 0 has no table",
            ));
        }
        if output_channel_count == 0 {
            return Err(Error::InvalidCombination(
                "Opus output channel count must not be zero",
            ));
        }
        if self.stream_count == 0 {
            return Err(Error::InvalidCombination(
                "Opus stream count must not be zero",
            ));
        }
        if self.coupled_count > self.stream_count {
            return Err(Error::InvalidCombination(
                "Opus coupled count exceeds the stream count",
            ));
        }
        if self.decoded_channels() > 255 {
            return Err(Error::InvalidCombination(
                "Opus streams decode to more than 255 channels",
            ));
        }

        Ok(())
    }

    // The checks on the table itself: its length, then, in every family but
    // the demixing matrix, that each entry names a decoded channel or silence.
    fn validate_table(&self, output_channel_count: u8) -> Result<()> {
        if self.channel_mapping.len() != self.table_len(output_channel_count) {
            return Err(self.table_len_error());
        }
        let decoded = self.decoded_channels();
        if self.family != Self::DEMIXING_MATRIX
            && self
                .channel_mapping
                .iter()
                .any(|&index| index != 255 && u16::from(index) >= decoded)
        {
            return Err(Error::InvalidCombination(
                "Opus channel mapping names a channel past the decoded ones",
            ));
        }

        Ok(())
    }

    /// The refusal of a table that does not hold [`Self::table_len`] octets.
    fn table_len_error(&self) -> Error {
        Error::InvalidCombination(if self.family == Self::DEMIXING_MATRIX {
            "Opus demixing matrix needs two octets per output and decoded channel"
        } else {
            "Opus channel mapping needs one entry per output channel"
        })
    }
}

impl Atom for Dops {
    const KIND: FourCC = FourCC::new(b"dOps");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let version = u8::decode(buf)?;
        if version != 0 {
            return Err(Error::UnknownVersion(version));
        }

        let output_channel_count = u8::decode(buf)?;
        let pre_skip = u16::decode(buf)?;
        let input_sample_rate = u32::decode(buf)?;
        let output_gain = i16::decode(buf)?;

        let family = u8::decode(buf)?;
        let channel_mapping = match family {
            0 => None,
            family => {
                let mut mapping = OpusChannelMapping {
                    family,
                    stream_count: u8::decode(buf)?,
                    coupled_count: u8::decode(buf)?,
                    channel_mapping: Vec::new(),
                };
                mapping.validate_counts(output_channel_count)?;
                // At most 2 × 255 × 255 octets now that the counts are checked,
                // and read only once the box is seen to hold them. The buffer
                // ends where the box does, so fewer octets mean a table cut
                // short, as libavformat's MP4 muxer cuts a family-3 matrix to
                // one octet per output channel.
                let len = mapping.table_len(output_channel_count);
                if buf.remaining() < len {
                    return Err(mapping.table_len_error());
                }
                mapping.channel_mapping = Vec::decode_exact(buf, len)?;
                mapping.validate_table(output_channel_count)?;
                Some(mapping)
            }
        };

        Ok(Self {
            output_channel_count,
            pre_skip,
            input_sample_rate,
            output_gain,
            channel_mapping,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        if let Some(mapping) = &self.channel_mapping {
            mapping.validate(self.output_channel_count)?;
        }

        (0u8).encode(buf)?;
        self.output_channel_count.encode(buf)?;
        self.pre_skip.encode(buf)?;
        self.input_sample_rate.encode(buf)?;
        self.output_gain.encode(buf)?;

        match &self.channel_mapping {
            None => (0u8).encode(buf)?,
            Some(mapping) => {
                mapping.family.encode(buf)?;
                mapping.stream_count.encode(buf)?;
                mapping.coupled_count.encode(buf)?;
                mapping.channel_mapping.encode(buf)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A stereo dOps with family 0 and no table.
    const ENCODED_DOPS_STEREO: &[u8] = &[
        0x00, 0x00, 0x00, 0x13, b'd', b'O', b'p', b's', 0x00, 0x02, 0x01, 0x38, 0x00, 0x00, 0xbb,
        0x80, 0xff, 0xfe, 0x00,
    ];

    // A 5.1 dOps with family 1 and the Vorbis order table libopus uses.
    const ENCODED_DOPS_SURROUND: &[u8] = &[
        0x00, 0x00, 0x00, 0x1b, b'd', b'O', b'p', b's', 0x00, 0x06, 0x01, 0x38, 0x00, 0x00, 0xbb,
        0x80, 0x00, 0x00, 0x01, 0x04, 0x02, 0x00, 0x04, 0x01, 0x02, 0x03, 0x05,
    ];

    fn surround() -> Dops {
        Dops {
            output_channel_count: 6,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: 0,
            channel_mapping: Some(OpusChannelMapping {
                family: 1,
                stream_count: 4,
                coupled_count: 2,
                channel_mapping: vec![0, 4, 1, 2, 3, 5],
            }),
        }
    }

    /// A first-order field with a head-locked stereo pair in family 2, as
    /// libopus's surround encoder lays it out: the pair is the one coupled
    /// stream, so it comes first in the decoded channels and last in the
    /// output ones.
    fn ambisonic_head_locked() -> Dops {
        Dops {
            output_channel_count: 6,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: 0,
            channel_mapping: Some(OpusChannelMapping {
                family: 2,
                stream_count: 5,
                coupled_count: 1,
                channel_mapping: vec![2, 3, 4, 5, 0, 1],
            }),
        }
    }

    /// A first-order field in family 3: two coupled streams, and a 4 × 4
    /// demixing matrix of Q15 coefficients (32 octets) in place of the
    /// indices.
    fn demixed() -> Dops {
        let matrix: Vec<i16> = (0..16i16)
            .map(|i| if i % 5 == 0 { i16::MAX } else { -i * 1000 })
            .collect();
        Dops {
            output_channel_count: 4,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: 0,
            channel_mapping: Some(OpusChannelMapping {
                family: OpusChannelMapping::DEMIXING_MATRIX,
                stream_count: 2,
                coupled_count: 2,
                channel_mapping: matrix.iter().flat_map(|c| c.to_le_bytes()).collect(),
            }),
        }
    }

    fn decode(bytes: &[u8]) -> Result<Dops> {
        Dops::decode(&mut std::io::Cursor::new(bytes))
    }

    fn encode(dops: &Dops) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        dops.encode(&mut buf)?;
        Ok(buf)
    }

    #[test]
    fn test_dops_family_0() {
        let dops = Dops {
            output_channel_count: 2,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: -2,
            channel_mapping: None,
        };

        assert_eq!(decode(ENCODED_DOPS_STEREO).unwrap(), dops);
        assert_eq!(encode(&dops).unwrap(), ENCODED_DOPS_STEREO);
    }

    #[test]
    fn test_dops_family_1() {
        let dops = surround();

        assert_eq!(decode(ENCODED_DOPS_SURROUND).unwrap(), dops);
        assert_eq!(encode(&dops).unwrap(), ENCODED_DOPS_SURROUND);
    }

    /// Family 2 keeps its table byte for byte, the pair's indices included.
    #[test]
    fn test_dops_family_2() {
        let dops = ambisonic_head_locked();
        let encoded = encode(&dops).unwrap();

        assert_eq!(
            &encoded[18..],
            &[0x02, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05, 0x00, 0x01]
        );
        assert_eq!(decode(&encoded).unwrap(), dops);
    }

    /// Family 3's table is the stream counts and the whole demixing matrix,
    /// the octet string of RFC 8486 §3.2.
    #[test]
    fn test_dops_family_3_demixing_matrix() {
        let dops = demixed();
        let encoded = encode(&dops).unwrap();

        assert_eq!(encoded.len(), 8 + 11 + 2 + 32);
        assert_eq!(&encoded[18..21], &[0x03, 0x02, 0x02]);
        assert_eq!(&encoded[21..23], &i16::MAX.to_le_bytes());
        assert_eq!(decode(&encoded).unwrap(), dops);
    }

    /// A family-3 box whose table holds one octet per output channel, as
    /// libavformat's MP4 muxer cuts a copied demixing matrix down to, is
    /// refused rather than read as a matrix it does not hold.
    #[test]
    fn test_dops_family_3_cut_matrix() {
        let full = encode(&demixed()).unwrap();
        let mut bytes = full[..21 + 4].to_vec();
        bytes[3] = bytes.len() as u8;

        assert!(matches!(
            decode(&bytes),
            Err(Error::InvalidCombination(reason)) if reason.starts_with("Opus demixing matrix")
        ));

        let mut dops = demixed();
        dops.channel_mapping
            .as_mut()
            .unwrap()
            .channel_mapping
            .truncate(4);
        assert!(matches!(encode(&dops), Err(Error::InvalidCombination(_))));
    }

    /// Family 3's matrix holds coefficients, not channel indices, so any
    /// value is kept.
    #[test]
    fn test_dops_family_3_takes_any_coefficient() {
        let mut dops = demixed();
        dops.channel_mapping
            .as_mut()
            .unwrap()
            .channel_mapping
            .fill(0xff);

        let encoded = encode(&dops).unwrap();
        assert_eq!(decode(&encoded).unwrap(), dops);
    }

    #[test]
    fn test_dops_any_family() {
        for family in [2, 4, 255] {
            let mut dops = surround();
            dops.channel_mapping.as_mut().unwrap().family = family;

            let encoded = encode(&dops).unwrap();
            assert_eq!(decode(&encoded).unwrap(), dops);
        }
    }

    #[test]
    fn test_dops_silent_channel() {
        let mut dops = surround();
        dops.channel_mapping.as_mut().unwrap().channel_mapping[5] = 255;

        let encoded = encode(&dops).unwrap();
        assert_eq!(decode(&encoded).unwrap(), dops);
    }

    #[test]
    fn test_dops_truncated_table() {
        // Drop the last table entry, leaving five for six output channels.
        let mut bytes = ENCODED_DOPS_SURROUND[..ENCODED_DOPS_SURROUND.len() - 1].to_vec();
        bytes[3] -= 1;

        assert!(matches!(
            decode(&bytes),
            Err(Error::InvalidCombination(reason)) if reason.contains("one entry per output")
        ));
    }

    /// The counts are checked before the table's length is taken from them,
    /// so a box whose counts are invalid is refused for its counts, however
    /// little of a table follows.
    #[test]
    fn test_dops_counts_checked_before_table() {
        let full = encode(&demixed()).unwrap();
        let mut bytes = full[..21].to_vec();
        bytes[3] = bytes.len() as u8;
        bytes[19] = 255;
        bytes[20] = 255;

        assert!(matches!(
            decode(&bytes),
            Err(Error::InvalidCombination(reason)) if reason.starts_with("Opus streams decode")
        ));
    }

    #[test]
    fn test_dops_rejects_invalid_table() {
        let invalid: [fn(&mut OpusChannelMapping); 4] = [
            |mapping| mapping.stream_count = 0,
            |mapping| mapping.coupled_count = mapping.stream_count + 1,
            |mapping| mapping.channel_mapping[0] = 6,
            |mapping| {
                mapping.stream_count = 200;
                mapping.coupled_count = 100;
            },
        ];

        for corrupt in invalid {
            let mut dops = surround();
            corrupt(dops.channel_mapping.as_mut().unwrap());
            assert!(matches!(encode(&dops), Err(Error::InvalidCombination(_))));

            // Write the same table past the encoder's check to prove decode refuses it too.
            let mapping = dops.channel_mapping.as_ref().unwrap();
            let mut bytes = ENCODED_DOPS_SURROUND.to_vec();
            bytes[19] = mapping.stream_count;
            bytes[20] = mapping.coupled_count;
            bytes[21..].copy_from_slice(&mapping.channel_mapping);
            assert!(matches!(decode(&bytes), Err(Error::InvalidCombination(_))));
        }
    }

    #[test]
    fn test_dops_rejects_family_0_table() {
        let mut dops = surround();
        dops.channel_mapping.as_mut().unwrap().family = 0;

        assert!(matches!(encode(&dops), Err(Error::InvalidCombination(_))));
    }

    #[test]
    fn test_dops_rejects_table_length_mismatch() {
        let mut dops = surround();
        dops.output_channel_count = 5;

        assert!(matches!(encode(&dops), Err(Error::InvalidCombination(_))));
    }

    /// An ambisonic Opus entry keeps its family-2 `dOps` and the `SA3D` box
    /// beside it, written in libavformat's order: the codec configuration
    /// first.
    #[test]
    fn test_opus_ambisonic_entry() {
        let opus = Opus {
            audio: Audio {
                data_reference_index: 1,
                channel_count: 6,
                sample_size: 16,
                sample_rate: 48000.into(),
            },
            dops: ambisonic_head_locked(),
            sa3d: Some(Sa3d {
                head_locked_stereo: true,
                ambisonic_order: 1,
                channel_map: (0..6).collect(),
                ..Default::default()
            }),
            btrt: None,
        };
        let mut buf = Vec::new();
        opus.encode(&mut buf).unwrap();

        let dops_at = buf.windows(4).position(|w| w == b"dOps").unwrap();
        let sa3d_at = buf.windows(4).position(|w| w == b"SA3D").unwrap();
        assert!(dops_at < sa3d_at);
        assert_eq!(Opus::decode(&mut buf.as_slice()).unwrap(), opus);
    }
}
