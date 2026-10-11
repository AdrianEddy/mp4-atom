use crate::*;

/// QuickTime base media information header (`gmhd`): the media header of a
/// QuickTime track that is neither video nor sound — timecode (`tmcd`), text and
/// GoPro metadata tracks (QTFF, "Base Media Information Header Atoms").
///
/// A QuickTime timecode track carries a `gmin` and a `tmcd` container holding the
/// timecode media information (`tcmi`). Other children (the undocumented `text`
/// atom chapter tracks carry, a GoPro `gpmd`) are skipped on decode.
///
/// Decoding is lenient by design: writers disagree on these boxes' layouts (see
/// [`Tcmi`]), and nothing in them is needed to read the track's samples, so a
/// child this decoder cannot read is logged and left out — never an error that
/// would fail the whole `moov`, as an unrecognised `gmhd` never did. A missing
/// or unreadable `gmin` reads as [`Gmin::default`].
///
/// The children are decoded here rather than through [`Any`]: the `tmcd`
/// container inside `gmhd` shares its FourCC with the `tmcd` *sample entry* in
/// `stsd`, and `Any` resolves a FourCC to exactly one type.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Gmhd {
    pub gmin: Gmin,
    /// The timecode media information (`tmcd` → `tcmi`), present on a timecode
    /// track.
    pub tcmi: Option<Tcmi>,
}

impl Atom for Gmhd {
    const KIND: FourCC = FourCC::new(b"gmhd");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let mut gmin = None;
        let mut tcmi = None;
        while let Some((kind, body)) = next_child(buf) {
            match kind {
                k if k == Gmin::KIND => match Gmin::decode_body(&mut body.as_slice()) {
                    Ok(g) => gmin = Some(g),
                    Err(e) => tracing::warn!(%e, "gmhd: unreadable gmin, left out"),
                },
                k if k == FourCC::new(b"tmcd") => {
                    let mut inner = body.as_slice();
                    while let Some((kind, body)) = next_child(&mut inner) {
                        if kind != Tcmi::KIND {
                            tracing::debug!(%kind, "gmhd/tmcd: skipping child box");
                            continue;
                        }
                        match Tcmi::decode_body(&mut body.as_slice()) {
                            Ok(t) => tcmi = Some(t),
                            Err(e) => tracing::warn!(%e, "gmhd: unreadable tcmi, left out"),
                        }
                    }
                }
                kind => tracing::debug!(%kind, "gmhd: skipping child box"),
            }
        }
        Ok(Self {
            gmin: gmin.unwrap_or_default(),
            tcmi,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.gmin.encode(buf)?;
        if let Some(tcmi) = &self.tcmi {
            buf.encode_atom(FourCC::new(b"tmcd"), |buf| tcmi.encode(buf))?;
        }
        Ok(())
    }
}

/// Split the next child box off `buf`: its kind and a copy of its body (these
/// boxes are a few dozen bytes). `None` at the end of the parent — and, having
/// consumed the rest, on a sub-header remainder (the QuickTime zero terminator)
/// or a child whose size runs past its parent.
fn next_child<B: Buf>(buf: &mut B) -> Option<(FourCC, Vec<u8>)> {
    let header = match buf.remaining() {
        n if n < 8 => None,
        _ => Header::decode(buf).ok(),
    };
    let Some(header) = header else {
        buf.advance(buf.remaining());
        return None;
    };
    let size = header.size.unwrap_or(buf.remaining());
    if size > buf.remaining() {
        tracing::warn!(kind = %header.kind, "gmhd: child box runs past its parent, rest skipped");
        buf.advance(buf.remaining());
        return None;
    }
    let body = buf.slice(size).to_vec();
    buf.advance(size);
    Some((header.kind, body))
}

/// Generic media information (`gmin`), a FullBox (QTFF): the graphics transfer
/// mode, its operation colour, and the audio balance.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Gmin {
    pub graphics_mode: u16,
    pub op_color: [u16; 3],
    pub balance: i16,
}

impl Default for Gmin {
    /// The values QuickTime and FFmpeg write: dither copy (`0x40`), mid-grey
    /// operation colour, centred balance.
    fn default() -> Self {
        Self {
            graphics_mode: 0x40,
            op_color: [0x8000; 3],
            balance: 0,
        }
    }
}

impl AtomExt for Gmin {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"gmin");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        let graphics_mode = u16::decode(buf)?;
        let op_color = [u16::decode(buf)?, u16::decode(buf)?, u16::decode(buf)?];
        let balance = i16::decode(buf)?;
        // The trailing reserved field — which some writers (GoPro) leave out.
        buf.advance(buf.remaining());
        Ok(Self {
            graphics_mode,
            op_color,
            balance,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.graphics_mode.encode(buf)?;
        for c in self.op_color {
            c.encode(buf)?;
        }
        self.balance.encode(buf)?;
        0u16.encode(buf)?; // reserved
        Ok(())
    }
}

/// Timecode media information (`tcmi`), a FullBox (QTFF): how a player draws the
/// timecode when the track is shown — font, face, size and colours.
///
/// Two layouts are in the wild. QTFF's: font, face, size, the two colours, then
/// the font name — GoPro writes it without the name. Apple's, which FFmpeg's
/// `movenc` copies: an extra 16-bit field after the size. Both decode; the
/// encoder writes Apple's.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Tcmi {
    pub text_font: u16,
    pub text_face: u16,
    pub text_size: u16,
    pub text_color: [u16; 3],
    pub background_color: [u16; 3],
    /// A Pascal string on the wire (one length byte, at most 255 bytes).
    pub font_name: String,
}

impl Default for Tcmi {
    /// What FFmpeg's `movenc` writes: 12 pt black on white, Lucida Grande.
    fn default() -> Self {
        Self {
            text_font: 0,
            text_face: 0,
            text_size: 12,
            text_color: [0; 3],
            background_color: [0xffff; 3],
            font_name: "Lucida Grande".into(),
        }
    }
}

impl AtomExt for Tcmi {
    type Ext = ();

    const KIND_EXT: FourCC = FourCC::new(b"tcmi");

    fn decode_body_ext<B: Buf>(buf: &mut B, _ext: ()) -> Result<Self> {
        let text_font = u16::decode(buf)?;
        let text_face = u16::decode(buf)?;
        let text_size = u16::decode(buf)?;
        // Apple's extra field: present exactly when the rest reads as it plus
        // the 12 colour bytes plus a Pascal name that ends at the box's end.
        let rest = buf.slice(buf.remaining());
        if rest.len() >= 15 && rest[14] as usize == rest.len() - 15 {
            buf.advance(2);
        }
        let text_color = [u16::decode(buf)?, u16::decode(buf)?, u16::decode(buf)?];
        let background_color = [u16::decode(buf)?, u16::decode(buf)?, u16::decode(buf)?];
        let font_name = if buf.has_remaining() {
            let len = (u8::decode(buf)? as usize).min(buf.remaining());
            let name = String::from_utf8_lossy(buf.slice(len)).into_owned();
            buf.advance(len);
            name
        } else {
            String::new()
        };
        buf.advance(buf.remaining());
        Ok(Self {
            text_font,
            text_face,
            text_size,
            text_color,
            background_color,
            font_name,
        })
    }

    fn encode_body_ext<B: BufMut>(&self, buf: &mut B) -> Result<()> {
        self.text_font.encode(buf)?;
        self.text_face.encode(buf)?;
        self.text_size.encode(buf)?;
        0u16.encode(buf)?; // reserved
        for c in self.text_color.iter().chain(&self.background_color) {
            c.encode(buf)?;
        }
        let name = self.font_name.as_bytes();
        let len = name.len().min(255);
        (len as u8).encode(buf)?;
        buf.append_slice(&name[..len]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `gmhd` FFmpeg's `movenc` writes for a MOV timecode track, minus the
    /// undocumented `text` child (skipped on decode, never written).
    fn ffmpeg_timecode_gmhd() -> Vec<u8> {
        let mut tcmi = vec![0, 0, 0, 0]; // version + flags
        tcmi.extend_from_slice(&[0, 0, 0, 0, 0, 12, 0, 0]); // font, face, size, reserved
        tcmi.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // text colour
        tcmi.extend_from_slice(&[0xff; 6]); // background colour
        tcmi.push(13);
        tcmi.extend_from_slice(b"Lucida Grande");
        let tcmi_box = [&((tcmi.len() + 8) as u32).to_be_bytes()[..], b"tcmi", &tcmi].concat();
        let tmcd_box = [
            &((tcmi_box.len() + 8) as u32).to_be_bytes()[..],
            b"tmcd",
            &tcmi_box,
        ]
        .concat();
        let gmin_box = [
            &0x18u32.to_be_bytes()[..],
            b"gmin",
            &[0, 0, 0, 0, 0, 0x40, 0x80, 0, 0x80, 0, 0x80, 0, 0, 0, 0, 0],
        ]
        .concat();
        let body = [gmin_box, tmcd_box].concat();
        [&((body.len() + 8) as u32).to_be_bytes()[..], b"gmhd", &body].concat()
    }

    #[test]
    fn timecode_gmhd_round_trips() {
        let expected = Gmhd {
            gmin: Gmin::default(),
            tcmi: Some(Tcmi::default()),
        };
        let mut buf = Vec::new();
        expected.encode(&mut buf).unwrap();
        assert_eq!(buf, ffmpeg_timecode_gmhd());
        let decoded = Gmhd::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn gopro_short_layouts_decode() {
        // A GoPro Fusion `.mov`: a 22-byte `gmin` without its reserved field and
        // an 18-byte QTFF `tcmi` without the font name.
        let hex = "00000044676d686400000016676d696e000000000000000000000000000000000026746d63640000001e74636d6900000000000000000000000000000000000000000000";
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let decoded = Gmhd::decode(&mut bytes.as_slice()).unwrap();
        assert_eq!(decoded.gmin.graphics_mode, 0);
        let tcmi = decoded.tcmi.unwrap();
        assert_eq!(
            (
                tcmi.text_size,
                tcmi.background_color,
                tcmi.font_name.as_str()
            ),
            (0, [0; 3], "")
        );
    }

    #[test]
    fn malformed_children_are_left_out() {
        // A `tmcd` child claiming more bytes than its parent holds.
        let mut body = Vec::new();
        body.extend_from_slice(&0x40u32.to_be_bytes());
        body.extend_from_slice(b"tmcd");
        body.extend_from_slice(&[0; 4]);
        let gmhd = [&((body.len() + 8) as u32).to_be_bytes()[..], b"gmhd", &body].concat();
        let decoded = Gmhd::decode(&mut gmhd.as_slice()).unwrap();
        assert_eq!(decoded, Gmhd::default());
    }

    #[test]
    fn unknown_children_are_skipped() {
        // gmin + the 0x2C-byte `text` atom FFmpeg writes + tmcd/tcmi.
        let reference = ffmpeg_timecode_gmhd();
        let mut text = 0x2Cu32.to_be_bytes().to_vec();
        text.extend_from_slice(b"text");
        text.resize(0x2C, 0);
        let gmin_end = 8 + 0x18;
        let body = [&reference[8..gmin_end], &text[..], &reference[gmin_end..]].concat();
        let with_text = [&((body.len() + 8) as u32).to_be_bytes()[..], b"gmhd", &body].concat();
        let decoded = Gmhd::decode(&mut with_text.as_slice()).unwrap();
        assert_eq!(decoded.tcmi, Some(Tcmi::default()));
    }
}
