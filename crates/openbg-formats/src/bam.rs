use std::borrow::Cow;
use std::io::Read;

use flate2::read::ZlibDecoder;

use crate::reader::Reader;
use crate::FormatError;

const FRAME_ENTRY_SIZE: usize = 12;
const CYCLE_ENTRY_SIZE: usize = 4;
const PALETTE_SIZE: usize = 256 * 4;
const MAX_BAM_BYTES: usize = 128 * 1024 * 1024;
const MAX_FRAME_PIXELS: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BamFrame {
    pub width: u16,
    pub height: u16,
    pub center_x: i16,
    pub center_y: i16,
    pub indices: Vec<u8>,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BamCycle {
    pub frame_indices: Vec<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bam {
    pub frames: Vec<BamFrame>,
    pub cycles: Vec<BamCycle>,
    pub palette: Vec<[u8; 4]>,
    pub transparent_index: u8,
}

impl Bam {
    /// Parses palette-based `BAM V1` and zlib-wrapped `BAMC V1` resources.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for unsupported headers, unsafe dimensions,
    /// malformed tables, invalid frame indices, or truncated RLE data.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let decoded = decompress_if_needed(bytes)?;
        let bytes = decoded.as_ref();
        let reader = Reader::new(bytes, "BAM V1");
        reader.expect(0, b"BAM ")?;
        reader.expect(4, b"V1  ")?;
        let frame_count = usize::from(reader.u16(8)?);
        let cycle_count = usize::from(reader.slice(0x0a, 1)?[0]);
        let compressed_index = reader.slice(0x0b, 1)?[0];
        let frame_offset = reader.usize32(0x0c)?;
        let palette_offset = reader.usize32(0x10)?;
        let lookup_offset = reader.usize32(0x14)?;
        reader.records(frame_offset, frame_count, FRAME_ENTRY_SIZE)?;
        let cycle_offset = frame_offset
            .checked_add(
                frame_count
                    .checked_mul(FRAME_ENTRY_SIZE)
                    .ok_or_else(|| FormatError::new("BAM V1", "cycle table offset overflow"))?,
            )
            .ok_or_else(|| FormatError::new("BAM V1", "cycle table offset overflow"))?;
        reader.records(cycle_offset, cycle_count, CYCLE_ENTRY_SIZE)?;
        let palette = reader.slice(palette_offset, PALETTE_SIZE)?;
        let transparent_index = palette
            .chunks_exact(4)
            .position(|color| color[0] == 0 && color[1] == 255 && color[2] == 0)
            .unwrap_or(0);
        let transparent_index = u8::try_from(transparent_index)
            .map_err(|_| FormatError::new("BAM V1", "transparent index exceeds u8"))?;

        let mut cycles = Vec::with_capacity(cycle_count);
        for cycle in 0..cycle_count {
            let offset = cycle_offset + cycle * CYCLE_ENTRY_SIZE;
            let count = usize::from(reader.u16(offset)?);
            let first = usize::from(reader.u16(offset + 2)?);
            let lookup_start =
                lookup_offset
                    .checked_add(first.checked_mul(2).ok_or_else(|| {
                        FormatError::new("BAM V1", "frame lookup offset overflow")
                    })?)
                    .ok_or_else(|| FormatError::new("BAM V1", "frame lookup offset overflow"))?;
            reader.records(lookup_start, count, 2)?;
            let mut frame_indices = Vec::with_capacity(count);
            for index in 0..count {
                let frame_index = reader.u16(lookup_start + index * 2)?;
                if usize::from(frame_index) >= frame_count {
                    return Err(FormatError::new(
                        "BAM V1",
                        format!("cycle {cycle} references missing frame {frame_index}"),
                    ));
                }
                frame_indices.push(frame_index);
            }
            cycles.push(BamCycle { frame_indices });
        }

        let rgba_palette = decode_palette(palette, transparent_index);
        let mut frames = Vec::with_capacity(frame_count);
        for frame in 0..frame_count {
            let offset = frame_offset + frame * FRAME_ENTRY_SIZE;
            let width = reader.u16(offset)?;
            let height = reader.u16(offset + 2)?;
            let pixel_count = usize::from(width)
                .checked_mul(usize::from(height))
                .ok_or_else(|| FormatError::new("BAM V1", "frame dimensions overflow"))?;
            if pixel_count > MAX_FRAME_PIXELS {
                return Err(FormatError::new(
                    "BAM V1",
                    format!("frame {frame} has {pixel_count} pixels; limit is {MAX_FRAME_PIXELS}"),
                ));
            }
            let data_field = reader.u32(offset + 8)?;
            let data_offset = usize::try_from(data_field & 0x7fff_ffff)
                .map_err(|_| FormatError::new("BAM V1", "frame offset does not fit usize"))?;
            let indices = if data_field & 0x8000_0000 != 0 {
                reader.slice(data_offset, pixel_count)?.to_vec()
            } else {
                decode_rle(bytes, data_offset, pixel_count, compressed_index)?
            };
            frames.push(BamFrame {
                width,
                height,
                center_x: reader.i16(offset + 4)?,
                center_y: reader.i16(offset + 6)?,
                rgba: apply_palette(&indices, &rgba_palette),
                indices,
            });
        }
        Ok(Self {
            frames,
            cycles,
            palette: rgba_palette,
            transparent_index,
        })
    }
}

fn decompress_if_needed(bytes: &[u8]) -> Result<Cow<'_, [u8]>, FormatError> {
    if bytes.get(0..4) != Some(b"BAMC") {
        return Ok(Cow::Borrowed(bytes));
    }
    let reader = Reader::new(bytes, "BAMC V1");
    reader.expect(4, b"V1  ")?;
    let expected = reader.usize32(8)?;
    if expected > MAX_BAM_BYTES {
        return Err(FormatError::new(
            "BAMC V1",
            format!("decoded size {expected} exceeds limit {MAX_BAM_BYTES}"),
        ));
    }
    let mut decoded = Vec::with_capacity(expected);
    ZlibDecoder::new(reader.slice(12, bytes.len().saturating_sub(12))?)
        .take((MAX_BAM_BYTES + 1) as u64)
        .read_to_end(&mut decoded)
        .map_err(|error| FormatError::new("BAMC V1", format!("zlib decode: {error}")))?;
    if decoded.len() != expected {
        return Err(FormatError::new(
            "BAMC V1",
            format!(
                "decoded {} bytes; header declares {expected}",
                decoded.len()
            ),
        ));
    }
    Ok(Cow::Owned(decoded))
}

fn decode_rle(
    bytes: &[u8],
    start: usize,
    pixel_count: usize,
    compressed_index: u8,
) -> Result<Vec<u8>, FormatError> {
    let mut input = start;
    let mut output = Vec::with_capacity(pixel_count);
    while output.len() < pixel_count {
        let value = *bytes
            .get(input)
            .ok_or_else(|| FormatError::new("BAM V1 RLE", "truncated frame data"))?;
        input += 1;
        if value == compressed_index {
            let run = usize::from(
                *bytes
                    .get(input)
                    .ok_or_else(|| FormatError::new("BAM V1 RLE", "missing run length"))?,
            ) + 1;
            input += 1;
            if output.len().saturating_add(run) > pixel_count {
                return Err(FormatError::new("BAM V1 RLE", "run exceeds frame size"));
            }
            output.resize(output.len() + run, compressed_index);
        } else {
            output.push(value);
        }
    }
    Ok(output)
}

fn decode_palette(palette: &[u8], transparent_index: u8) -> Vec<[u8; 4]> {
    palette
        .chunks_exact(4)
        .enumerate()
        .map(|(index, color)| {
            [
                color[2],
                color[1],
                color[0],
                if index == usize::from(transparent_index) {
                    0
                } else {
                    255
                },
            ]
        })
        .collect()
}

/// Expands palette indices into RGBA pixels using a possibly remapped palette.
#[must_use]
pub fn apply_palette(indices: &[u8], palette: &[[u8; 4]]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(indices.len() * 4);
    for index in indices.iter().copied() {
        rgba.extend_from_slice(&palette[usize::from(index)]);
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::Bam;

    #[test]
    fn parses_an_uncompressed_frame_and_cycle() {
        let mut bytes = vec![0_u8; 1067];
        bytes[0..8].copy_from_slice(b"BAM V1  ");
        bytes[8..10].copy_from_slice(&1_u16.to_le_bytes());
        bytes[10] = 1;
        bytes[12..16].copy_from_slice(&24_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&40_u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&1064_u32.to_le_bytes());
        bytes[24..26].copy_from_slice(&1_u16.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[32..36].copy_from_slice(&(0x8000_0000_u32 | 1066).to_le_bytes());
        bytes[36..38].copy_from_slice(&1_u16.to_le_bytes());
        bytes[44..48].copy_from_slice(&[30, 20, 10, 0]);
        bytes[1066] = 1;

        let bam = Bam::parse(&bytes).expect("synthetic BAM is valid");
        assert_eq!(bam.cycles[0].frame_indices, vec![0]);
        assert_eq!(bam.frames[0].rgba, vec![10, 20, 30, 255]);
    }
}
