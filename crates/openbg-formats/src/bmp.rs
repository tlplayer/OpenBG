use crate::reader::Reader;
use crate::FormatError;

const FILE_HEADER_SIZE: usize = 14;
const INFO_HEADER_SIZE: usize = 40;
const MAX_PIXELS: usize = 64 * 1024 * 1024;

/// Uncompressed 8-bit BMP indices normalized to top-left row-major order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedBitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub palette: Vec<[u8; 4]>,
}

/// Uncompressed 24-bit BMP pixels normalized to top-left row-major RGBA.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RgbaBitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl RgbaBitmap {
    /// Parses an uncompressed 24-bit Windows BMP.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for unsupported headers, compression, bit
    /// depth, dimensions, or truncated pixel rows.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let reader = Reader::new(bytes, "24-bit BMP");
        reader.expect(0, b"BM")?;
        reader.slice(0, FILE_HEADER_SIZE + INFO_HEADER_SIZE)?;
        let data_offset = reader.usize32(10)?;
        if reader.u32(14)? < INFO_HEADER_SIZE as u32 {
            return Err(FormatError::new("24-bit BMP", "unsupported DIB header"));
        }
        let width = reader.i32(18)?;
        let signed_height = reader.i32(22)?;
        if width <= 0 || signed_height == 0 {
            return Err(FormatError::new(
                "24-bit BMP",
                "dimensions must be non-zero",
            ));
        }
        if reader.u16(26)? != 1 || reader.u16(28)? != 24 {
            return Err(FormatError::new(
                "24-bit BMP",
                "only single-plane 24-bit BMP is supported",
            ));
        }
        if reader.u32(30)? != 0 {
            return Err(FormatError::new(
                "24-bit BMP",
                "compressed BMP is unsupported",
            ));
        }
        let width = usize::try_from(width)
            .map_err(|_| FormatError::new("24-bit BMP", "width does not fit usize"))?;
        let height = usize::try_from(signed_height.unsigned_abs())
            .map_err(|_| FormatError::new("24-bit BMP", "height does not fit usize"))?;
        let pixel_count = width
            .checked_mul(height)
            .ok_or_else(|| FormatError::new("24-bit BMP", "pixel count overflow"))?;
        if pixel_count > MAX_PIXELS {
            return Err(FormatError::new(
                "24-bit BMP",
                format!("pixel count {pixel_count} exceeds limit {MAX_PIXELS}"),
            ));
        }
        let row_stride = width
            .checked_mul(3)
            .and_then(|bytes| bytes.checked_add(3))
            .map(|bytes| bytes & !3)
            .ok_or_else(|| FormatError::new("24-bit BMP", "row stride overflow"))?;
        reader.records(data_offset, height, row_stride)?;
        let top_down = signed_height < 0;
        let mut pixels = vec![[0_u8; 4]; pixel_count];
        for output_y in 0..height {
            let source_y = if top_down {
                output_y
            } else {
                height - 1 - output_y
            };
            let row = reader.slice(data_offset + source_y * row_stride, row_stride)?;
            for x in 0..width {
                let source = x * 3;
                pixels[output_y * width + x] = [row[source + 2], row[source + 1], row[source], 255];
            }
        }
        Ok(Self {
            width: u32::try_from(width)
                .map_err(|_| FormatError::new("24-bit BMP", "width exceeds u32"))?,
            height: u32::try_from(height)
                .map_err(|_| FormatError::new("24-bit BMP", "height exceeds u32"))?,
            pixels,
        })
    }
}

impl IndexedBitmap {
    /// Parses an uncompressed 8-bit Windows BMP.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for unsupported DIB variants, compression, bit
    /// depth, dimensions, or truncated pixel rows.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let reader = Reader::new(bytes, "BMP");
        reader.expect(0, b"BM")?;
        reader.slice(0, FILE_HEADER_SIZE + INFO_HEADER_SIZE)?;
        let data_offset = reader.usize32(10)?;
        let header_size = reader.u32(14)?;
        if header_size < INFO_HEADER_SIZE as u32 {
            return Err(FormatError::new(
                "BMP",
                format!("unsupported DIB header size {header_size}"),
            ));
        }
        let width = reader.i32(18)?;
        let signed_height = reader.i32(22)?;
        if width <= 0 || signed_height == 0 {
            return Err(FormatError::new("BMP", "dimensions must be non-zero"));
        }
        let planes = reader.u16(26)?;
        let bits_per_pixel = reader.u16(28)?;
        if planes != 1 || !matches!(bits_per_pixel, 4 | 8) {
            return Err(FormatError::new(
                "BMP",
                format!(
                    "only single-plane 4/8-bit indexed BMP is supported; got planes={planes}, bpp={bits_per_pixel}"
                ),
            ));
        }
        if reader.u32(30)? != 0 {
            return Err(FormatError::new("BMP", "compressed BMP is unsupported"));
        }
        let palette_entries = match reader.u32(46)? {
            0 => 1_usize << bits_per_pixel,
            count => usize::try_from(count)
                .map_err(|_| FormatError::new("BMP", "palette size exceeds usize"))?,
        };
        if palette_entries > 256 {
            return Err(FormatError::new(
                "BMP",
                format!("palette has {palette_entries} entries; limit is 256"),
            ));
        }
        let palette_offset = FILE_HEADER_SIZE
            .checked_add(
                usize::try_from(header_size)
                    .map_err(|_| FormatError::new("BMP", "DIB header size does not fit usize"))?,
            )
            .ok_or_else(|| FormatError::new("BMP", "palette offset overflow"))?;
        let raw_palette = reader.slice(palette_offset, palette_entries * 4)?;
        if palette_offset + raw_palette.len() > data_offset {
            return Err(FormatError::new("BMP", "palette overlaps pixel data"));
        }
        let palette = raw_palette
            .chunks_exact(4)
            .map(|color| [color[2], color[1], color[0], 255])
            .collect();
        let width = usize::try_from(width)
            .map_err(|_| FormatError::new("BMP", "width does not fit usize"))?;
        let height = usize::try_from(signed_height.unsigned_abs())
            .map_err(|_| FormatError::new("BMP", "height does not fit usize"))?;
        let pixel_count = width
            .checked_mul(height)
            .ok_or_else(|| FormatError::new("BMP", "pixel count overflow"))?;
        if pixel_count > MAX_PIXELS {
            return Err(FormatError::new(
                "BMP",
                format!("pixel count {pixel_count} exceeds limit {MAX_PIXELS}"),
            ));
        }
        let row_bits = width
            .checked_mul(usize::from(bits_per_pixel))
            .ok_or_else(|| FormatError::new("BMP", "row bit count overflow"))?;
        let row_stride = row_bits
            .checked_add(31)
            .map(|value| (value / 32) * 4)
            .ok_or_else(|| FormatError::new("BMP", "row stride overflow"))?;
        reader.records(data_offset, height, row_stride)?;
        let top_down = signed_height < 0;
        let mut pixels = vec![0_u8; pixel_count];
        for output_y in 0..height {
            let source_y = if top_down {
                output_y
            } else {
                height - 1 - output_y
            };
            let source = data_offset + source_y * row_stride;
            let destination = output_y * width;
            let row = reader.slice(source, row_stride)?;
            if bits_per_pixel == 8 {
                pixels[destination..destination + width].copy_from_slice(&row[..width]);
            } else {
                for x in 0..width {
                    let packed = row[x / 2];
                    pixels[destination + x] = if x % 2 == 0 {
                        packed >> 4
                    } else {
                        packed & 0x0f
                    };
                }
            }
        }
        Ok(Self {
            width: u32::try_from(width)
                .map_err(|_| FormatError::new("BMP", "width exceeds u32"))?,
            height: u32::try_from(height)
                .map_err(|_| FormatError::new("BMP", "height exceeds u32"))?,
            pixels,
            palette,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{IndexedBitmap, RgbaBitmap};

    #[test]
    fn normalizes_bottom_up_rows_and_padding() {
        let mut bytes = vec![0_u8; 70];
        bytes[0..2].copy_from_slice(b"BM");
        bytes[10..14].copy_from_slice(&62_u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&3_i32.to_le_bytes());
        bytes[22..26].copy_from_slice(&2_i32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&8_u16.to_le_bytes());
        bytes[46..50].copy_from_slice(&2_u32.to_le_bytes());
        bytes[54..62].copy_from_slice(&[3, 2, 1, 0, 6, 5, 4, 0]);
        bytes[62..66].copy_from_slice(&[4, 5, 6, 0]);
        bytes[66..70].copy_from_slice(&[1, 2, 3, 0]);

        let bitmap = IndexedBitmap::parse(&bytes).expect("synthetic BMP is valid");
        assert_eq!((bitmap.width, bitmap.height), (3, 2));
        assert_eq!(bitmap.pixels, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(bitmap.palette, vec![[1, 2, 3, 255], [4, 5, 6, 255]]);
    }

    #[test]
    fn unpacks_four_bit_indices_high_nibble_first() {
        let mut bytes = vec![0_u8; 66];
        bytes[0..2].copy_from_slice(b"BM");
        bytes[10..14].copy_from_slice(&62_u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&3_i32.to_le_bytes());
        bytes[22..26].copy_from_slice(&1_i32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&4_u16.to_le_bytes());
        bytes[46..50].copy_from_slice(&2_u32.to_le_bytes());
        bytes[62..66].copy_from_slice(&[0x12, 0x30, 0, 0]);

        let bitmap = IndexedBitmap::parse(&bytes).expect("synthetic BMP is valid");
        assert_eq!(bitmap.pixels, vec![1, 2, 3]);
    }

    #[test]
    fn decodes_bottom_up_24_bit_rows() {
        let mut bytes = vec![0_u8; 62];
        bytes[0..2].copy_from_slice(b"BM");
        bytes[10..14].copy_from_slice(&54_u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&1_i32.to_le_bytes());
        bytes[22..26].copy_from_slice(&2_i32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&24_u16.to_le_bytes());
        bytes[54..58].copy_from_slice(&[6, 5, 4, 0]);
        bytes[58..62].copy_from_slice(&[3, 2, 1, 0]);

        let bitmap = RgbaBitmap::parse(&bytes).expect("synthetic 24-bit BMP is valid");
        assert_eq!((bitmap.width, bitmap.height), (1, 2));
        assert_eq!(bitmap.pixels, vec![[1, 2, 3, 255], [4, 5, 6, 255]]);
    }
}
