use crate::*;

// See ETSI TS 102 366 V1.4.1 (2017-09) for details of AC-3 and EAC-3

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Ac3 {
    pub audio: Audio,
    pub dac3: Ac3SpecificBox,
}

impl Atom for Ac3 {
    const KIND: FourCC = FourCC::new(b"ac-3");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let audio = Audio::decode(buf)?;

        let mut dac3 = None;

        // The `dac3` sits directly under `ac-3` (ISO-BMFF) or nested inside a
        // QuickTime `wave` (siDecompressionParam) box (QTFF).
        while let Some(atom) = Any::decode_maybe(buf)? {
            match atom {
                Any::Ac3SpecificBox(atom) => dac3 = atom.into(),
                Any::Unknown(kind, body) if kind == WAVE => {
                    if let Some(atom) = wave_child::<Ac3SpecificBox>(&body)? {
                        dac3 = Some(atom);
                    }
                }
                unknown => Self::decode_unknown(&unknown)?,
            }
        }
        skip_trailing_padding(buf);

        Ok(Self {
            audio,
            dac3: dac3.ok_or(Error::MissingBox(Ac3SpecificBox::KIND))?,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.audio.encode(buf)?;
        self.dac3.encode(buf)?;
        Ok(())
    }
}

/// QuickTime `wave` (siDecompressionParam) container FourCC.
pub(super) const WAVE: FourCC = FourCC::new(b"wave");

/// The `T` box a QuickTime `wave` body wraps, if any. Every other child (the
/// `frma` format box, the terminator) is skipped by size, not typed-decoded.
pub(super) fn wave_child<T: Atom>(mut wave: &[u8]) -> Result<Option<T>> {
    while let Some(header) = Header::decode_maybe(&mut wave)? {
        let size = match header.size {
            Some(size) if size <= wave.remaining() => size,
            _ => break,
        };
        if header.kind == T::KIND {
            return Ok(Some(T::decode_body(&mut wave.slice(size))?));
        }
        wave.advance(size);
    }
    Ok(None)
}

// AC-3 specific data
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Ac3SpecificBox {
    pub fscod: u8,
    pub bsid: u8,
    pub bsmod: u8,
    pub acmod: u8,
    pub lfeon: bool,
    pub bit_rate_code: u8,
}

impl Atom for Ac3SpecificBox {
    const KIND: FourCC = FourCC::new(b"dac3");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let body_bytes = u24::decode(buf)?;
        let body: u32 = body_bytes.into();
        let fscod = ((body >> 22) & 0b11) as u8;
        let bsid = ((body >> 17) & 0b11111) as u8;
        let bsmod = ((body >> 14) & 0b111) as u8;
        let acmod = ((body >> 11) & 0b111) as u8;
        let lfeon = ((body >> 10) & 0b1) == 0b1;
        let bit_rate_code = ((body >> 5) & 0b11111) as u8;
        Ok(Self {
            fscod,
            bsid,
            bsmod,
            acmod,
            lfeon,
            bit_rate_code,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        let body: u32 = ((self.bit_rate_code as u32) << 5)
            | ((self.acmod as u32) << 11)
            | ((self.bsmod as u32) << 14)
            | ((self.bsid as u32) << 17)
            | ((self.fscod as u32) << 22)
            | (if self.lfeon { 0x1u32 << 10 } else { 0u32 });
        let body_bytes: u24 = body
            .try_into()
            .map_err(|_| Error::TooLarge(Ac3SpecificBox::KIND))?;
        body_bytes.encode(buf)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac3_dac3_inside_a_quicktime_wave() {
        let dac3 = Ac3SpecificBox {
            fscod: 0,
            bsid: 8,
            bsmod: 0,
            acmod: 7,
            lfeon: true,
            bit_rate_code: 15,
        };
        let mut dac3_box = Vec::new();
        dac3.encode(&mut dac3_box).unwrap();
        let mut wave = Vec::new();
        wave.extend_from_slice(&((8 + 12 + dac3_box.len()) as u32).to_be_bytes());
        wave.extend_from_slice(b"wave");
        wave.extend_from_slice(&12u32.to_be_bytes());
        wave.extend_from_slice(b"frma");
        wave.extend_from_slice(b"ac-3");
        wave.extend_from_slice(&dac3_box);
        let audio = Audio {
            data_reference_index: 1,
            channel_count: 6,
            sample_size: 16,
            sample_rate: 48000.into(),
        };
        let mut body = Vec::new();
        audio.encode(&mut body).unwrap();
        body.extend_from_slice(&wave);
        let mut entry = Vec::new();
        entry.extend_from_slice(&((8 + body.len()) as u32).to_be_bytes());
        entry.extend_from_slice(b"ac-3");
        entry.extend_from_slice(&body);

        let ac3 = Ac3::decode(&mut std::io::Cursor::new(&entry)).expect("failed to decode ac-3");
        assert_eq!(ac3.dac3, dac3);
    }

    // Streaminfo metadata block only
    const ENCODED_AC3: &[u8] = &[
        0x00, 0x00, 0x00, 0x2f, 0x61, 0x63, 0x2d, 0x33, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x10, 0x00, 0x00,
        0x00, 0x00, 0xac, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0b, 0x64, 0x61, 0x63, 0x33, 0x50,
        0x11, 0x40,
    ];

    #[test]
    fn test_ac3_decode() {
        let buf: &mut std::io::Cursor<&[u8]> = &mut std::io::Cursor::new(ENCODED_AC3);

        let ac3 = Ac3::decode(buf).expect("failed to decode ac-3");

        assert_eq!(
            ac3,
            Ac3 {
                audio: Audio {
                    data_reference_index: 1,
                    channel_count: 2,
                    sample_size: 16,
                    sample_rate: 44100.into()
                },
                dac3: Ac3SpecificBox {
                    fscod: 1,
                    bsid: 8,
                    bsmod: 0,
                    acmod: 2,
                    lfeon: false,
                    bit_rate_code: 10
                }
            }
        );
    }

    #[test]
    fn test_ac3_encode() {
        let ac3 = Ac3 {
            audio: Audio {
                data_reference_index: 1,
                channel_count: 2,
                sample_size: 16,
                sample_rate: 44100.into(),
            },
            dac3: Ac3SpecificBox {
                fscod: 1,
                bsid: 8,
                bsmod: 0,
                acmod: 2,
                lfeon: false,
                bit_rate_code: 10,
            },
        };

        let mut buf = Vec::new();
        ac3.encode(&mut buf).unwrap();

        assert_eq!(buf.as_slice(), ENCODED_AC3);
    }
}
