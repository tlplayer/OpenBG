use openbg_domain::ResRef;

use crate::reader::Reader;
use crate::FormatError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaseOverlay {
    pub width: u16,
    pub height: u16,
    pub tileset: ResRef,
    pub tile_indices: Vec<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Wed {
    pub base: BaseOverlay,
}

impl Wed {
    /// Parses the base overlay from a `WED V1.3` resource.
    ///
    /// Animated cells use their first frame in this initial viewer.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for invalid headers, references, dimensions, or
    /// table bounds.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let reader = Reader::new(bytes, "WED V1.3");
        reader.expect(0, b"WED ")?;
        reader.expect(4, b"V1.3")?;
        let overlay_count = reader.usize32(8)?;
        if overlay_count == 0 {
            return Err(FormatError::new("WED V1.3", "area has no base overlay"));
        }
        let overlay_offset = reader.usize32(16)?;
        reader.slice(overlay_offset, 24)?;
        let width = reader.u16(overlay_offset)?;
        let height = reader.u16(overlay_offset + 2)?;
        let raw_tileset = reader.array::<8>(overlay_offset + 4)?;
        let length = raw_tileset.iter().position(|byte| *byte == 0).unwrap_or(8);
        let tileset_name = std::str::from_utf8(&raw_tileset[..length])
            .map_err(|_| FormatError::new("WED V1.3", "tileset resref is not ASCII"))?;
        let tileset = ResRef::new(tileset_name)
            .map_err(|error| FormatError::new("WED V1.3", error.to_string()))?;
        let tilemap_offset = reader.usize32(overlay_offset + 16)?;
        let lookup_offset = reader.usize32(overlay_offset + 20)?;
        let cell_count = usize::from(width)
            .checked_mul(usize::from(height))
            .ok_or_else(|| FormatError::new("WED V1.3", "base dimensions overflow"))?;
        reader.records(tilemap_offset, cell_count, 10)?;

        let mut tile_indices = Vec::with_capacity(cell_count);
        for cell in 0..cell_count {
            let cell_offset = tilemap_offset + cell * 10;
            let lookup_index = usize::from(reader.u16(cell_offset)?);
            let frame_count = reader.u16(cell_offset + 2)?;
            if frame_count == 0 {
                return Err(FormatError::new(
                    "WED V1.3",
                    format!("base cell {cell} has no tile frames"),
                ));
            }
            let index_offset =
                lookup_offset
                    .checked_add(lookup_index.checked_mul(2).ok_or_else(|| {
                        FormatError::new("WED V1.3", "tile lookup offset overflow")
                    })?)
                    .ok_or_else(|| FormatError::new("WED V1.3", "tile lookup offset overflow"))?;
            tile_indices.push(reader.u16(index_offset)?);
        }

        Ok(Self {
            base: BaseOverlay {
                width,
                height,
                tileset,
                tile_indices,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Wed;

    #[test]
    fn parses_a_synthetic_base_overlay() {
        let mut bytes = vec![0_u8; 68];
        bytes[0..8].copy_from_slice(b"WED V1.3");
        bytes[8..12].copy_from_slice(&1_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&32_u32.to_le_bytes());
        bytes[32..34].copy_from_slice(&1_u16.to_le_bytes());
        bytes[34..36].copy_from_slice(&1_u16.to_le_bytes());
        bytes[36..44].copy_from_slice(b"TESTTIS\0");
        bytes[48..52].copy_from_slice(&56_u32.to_le_bytes());
        bytes[52..56].copy_from_slice(&66_u32.to_le_bytes());
        bytes[58..60].copy_from_slice(&1_u16.to_le_bytes());
        bytes[66..68].copy_from_slice(&42_u16.to_le_bytes());

        let wed = Wed::parse(&bytes).expect("synthetic WED is valid");
        assert_eq!(wed.base.width, 1);
        assert_eq!(wed.base.height, 1);
        assert_eq!(wed.base.tileset.as_str(), "TESTTIS");
        assert_eq!(wed.base.tile_indices, vec![42]);
    }
}
