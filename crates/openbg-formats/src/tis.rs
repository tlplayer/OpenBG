use std::collections::BTreeMap;
use std::io::Read;

use flate2::read::ZlibDecoder;
use openbg_domain::ResRef;

use crate::{FormatError, ResourceData, Wed};

const TILE_DIMENSION: usize = 64;
const PALETTE_SIZE: usize = 256 * 4;
const PIXEL_COUNT: usize = TILE_DIMENSION * TILE_DIMENSION;
const PALETTE_TILE_SIZE: usize = PALETTE_SIZE + PIXEL_COUNT;
const MAX_IMAGE_BYTES: usize = 512 * 1024 * 1024;
const MAX_PAGE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Composes the static first frame of a WED base layer into one RGBA image.
///
/// # Errors
///
/// Returns [`FormatError`] for non-palette TIS data, missing tile indices, or
/// dimensions that exceed addressable memory.
pub fn compose_base_layer(wed: &Wed, resource: ResourceData<'_>) -> Result<RgbaImage, FormatError> {
    let ResourceData::Tileset {
        bytes,
        tile_count,
        tile_size,
    } = resource
    else {
        return Err(FormatError::new("TIS V1", "resource is not a BIFF tileset"));
    };
    if usize::try_from(tile_size).ok() != Some(PALETTE_TILE_SIZE) {
        return Err(FormatError::new(
            "TIS V1",
            format!(
                "tile block size {tile_size} is not palette TIS size {PALETTE_TILE_SIZE}; PVRZ TIS is the next viewer milestone"
            ),
        ));
    }

    let width = usize::from(wed.base.width)
        .checked_mul(TILE_DIMENSION)
        .ok_or_else(|| FormatError::new("TIS V1", "image width overflow"))?;
    let height = usize::from(wed.base.height)
        .checked_mul(TILE_DIMENSION)
        .ok_or_else(|| FormatError::new("TIS V1", "image height overflow"))?;
    let image_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| FormatError::new("TIS V1", "image allocation size overflow"))?;
    if image_len > MAX_IMAGE_BYTES {
        return Err(FormatError::new(
            "TIS V1",
            format!("composed image needs {image_len} bytes; limit is {MAX_IMAGE_BYTES}"),
        ));
    }

    let mut pixels = vec![0_u8; image_len];
    let mut decoded = BTreeMap::<u16, Vec<u8>>::new();
    for (cell, tile_index) in wed.base.tile_indices.iter().copied().enumerate() {
        if u32::from(tile_index) >= tile_count {
            return Err(FormatError::new(
                "TIS V1",
                format!("tile index {tile_index} exceeds tile count {tile_count}"),
            ));
        }
        let tile = if let Some(tile) = decoded.get(&tile_index) {
            tile
        } else {
            let tile = decode_tile(bytes, usize::from(tile_index))?;
            decoded.entry(tile_index).or_insert(tile)
        };
        let cell_x = (cell % usize::from(wed.base.width)) * TILE_DIMENSION;
        let cell_y = (cell / usize::from(wed.base.width)) * TILE_DIMENSION;
        for row in 0..TILE_DIMENSION {
            let source = row * TILE_DIMENSION * 4;
            let destination = ((cell_y + row) * width + cell_x) * 4;
            pixels[destination..destination + TILE_DIMENSION * 4]
                .copy_from_slice(&tile[source..source + TILE_DIMENSION * 4]);
        }
    }

    Ok(RgbaImage {
        width: u32::try_from(width)
            .map_err(|_| FormatError::new("TIS V1", "image width exceeds u32"))?,
        height: u32::try_from(height)
            .map_err(|_| FormatError::new("TIS V1", "image height exceeds u32"))?,
        pixels,
    })
}

/// Composes a PVRZ-backed Enhanced Edition TIS base layer.
///
/// `load_page` receives the generated PVRZ resource reference (for example,
/// `A260000` for page zero of `AR2600`) and returns its compressed bytes.
///
/// # Errors
///
/// Returns [`FormatError`] for invalid TIS entries, missing/malformed PVRZ
/// pages, unsupported PVR pixel formats, or unsafe image dimensions.
pub fn compose_base_layer_with_pages<F>(
    wed: &Wed,
    resource: ResourceData<'_>,
    mut load_page: F,
) -> Result<RgbaImage, FormatError>
where
    F: FnMut(&ResRef) -> Result<Vec<u8>, FormatError>,
{
    let ResourceData::Tileset {
        bytes,
        tile_count,
        tile_size,
    } = resource
    else {
        return Err(FormatError::new("TIS V1", "resource is not a BIFF tileset"));
    };
    if tile_size != 12 {
        return Err(FormatError::new(
            "TIS V1",
            format!("tile block size {tile_size} is not PVRZ TIS size 12"),
        ));
    }
    let dimensions = image_dimensions(wed)?;
    let mut pixels = vec![0_u8; dimensions.byte_len];
    let mut pages = BTreeMap::<u32, RgbaImage>::new();

    for (cell, tile_index) in wed.base.tile_indices.iter().copied().enumerate() {
        if u32::from(tile_index) >= tile_count {
            return Err(FormatError::new(
                "TIS V1",
                format!("tile index {tile_index} exceeds tile count {tile_count}"),
            ));
        }
        let offset = usize::from(tile_index) * 12;
        let entry = bytes
            .get(offset..offset + 12)
            .ok_or_else(|| FormatError::bounds("TIS V1 PVRZ tile", offset, 12, bytes.len()))?;
        let page = u32::from_le_bytes(
            entry[0..4]
                .try_into()
                .map_err(|_| FormatError::new("TIS V1", "invalid PVRZ page field"))?,
        );
        if page == u32::MAX {
            continue;
        }
        let source_x = usize::try_from(u32::from_le_bytes(
            entry[4..8]
                .try_into()
                .map_err(|_| FormatError::new("TIS V1", "invalid PVRZ X coordinate"))?,
        ))
        .map_err(|_| FormatError::new("TIS V1", "PVRZ X coordinate does not fit usize"))?;
        let source_y = usize::try_from(u32::from_le_bytes(
            entry[8..12]
                .try_into()
                .map_err(|_| FormatError::new("TIS V1", "invalid PVRZ Y coordinate"))?,
        ))
        .map_err(|_| FormatError::new("TIS V1", "PVRZ Y coordinate does not fit usize"))?;

        if let std::collections::btree_map::Entry::Vacant(entry) = pages.entry(page) {
            let page_name = pvrz_resref(&wed.base.tileset, page)?;
            let compressed = load_page(&page_name)?;
            entry.insert(decode_pvrz(&compressed)?);
        }
        let page_image = pages
            .get(&page)
            .ok_or_else(|| FormatError::new("PVRZ", "decoded page disappeared"))?;
        copy_tile(
            &mut pixels,
            dimensions.width,
            cell,
            usize::from(wed.base.width),
            page_image,
            source_x,
            source_y,
        )?;
    }

    Ok(RgbaImage {
        width: u32::try_from(dimensions.width)
            .map_err(|_| FormatError::new("TIS V1", "image width exceeds u32"))?,
        height: u32::try_from(dimensions.height)
            .map_err(|_| FormatError::new("TIS V1", "image height exceeds u32"))?,
        pixels,
    })
}

/// Generates the PVRZ resource reference for a TIS page.
///
/// # Errors
///
/// Returns [`FormatError`] when the tileset name is too short, the page exceeds
/// two decimal digits, or the generated name is not a valid resource reference.
pub fn pvrz_resref(tileset: &ResRef, page: u32) -> Result<ResRef, FormatError> {
    if page > 99 {
        return Err(FormatError::new("TIS V1", "PVRZ page exceeds 99"));
    }
    let name = tileset.as_str();
    if name.len() < 3 {
        return Err(FormatError::new(
            "TIS V1",
            "tileset resref is too short for a PVRZ page name",
        ));
    }
    let page_name = format!("{}{}{:02}", &name[..1], &name[2..], page);
    ResRef::new(page_name).map_err(|error| FormatError::new("TIS V1", error.to_string()))
}

struct ImageDimensions {
    width: usize,
    height: usize,
    byte_len: usize,
}

fn image_dimensions(wed: &Wed) -> Result<ImageDimensions, FormatError> {
    let width = usize::from(wed.base.width)
        .checked_mul(TILE_DIMENSION)
        .ok_or_else(|| FormatError::new("TIS V1", "image width overflow"))?;
    let height = usize::from(wed.base.height)
        .checked_mul(TILE_DIMENSION)
        .ok_or_else(|| FormatError::new("TIS V1", "image height overflow"))?;
    let byte_len = width
        .checked_mul(height)
        .and_then(|count| count.checked_mul(4))
        .ok_or_else(|| FormatError::new("TIS V1", "image allocation size overflow"))?;
    if byte_len > MAX_IMAGE_BYTES {
        return Err(FormatError::new(
            "TIS V1",
            format!("composed image needs {byte_len} bytes; limit is {MAX_IMAGE_BYTES}"),
        ));
    }
    Ok(ImageDimensions {
        width,
        height,
        byte_len,
    })
}

fn decode_pvrz(bytes: &[u8]) -> Result<RgbaImage, FormatError> {
    if bytes.len() < 6 {
        return Err(FormatError::new("PVRZ", "compressed page is too short"));
    }
    let expected = usize::try_from(u32::from_le_bytes(
        bytes[0..4]
            .try_into()
            .map_err(|_| FormatError::new("PVRZ", "invalid size prefix"))?,
    ))
    .map_err(|_| FormatError::new("PVRZ", "decoded size does not fit usize"))?;
    if expected > MAX_PAGE_BYTES {
        return Err(FormatError::new(
            "PVRZ",
            format!("decoded page size {expected} exceeds limit {MAX_PAGE_BYTES}"),
        ));
    }
    let mut decoded = Vec::with_capacity(expected);
    ZlibDecoder::new(&bytes[4..])
        .take((MAX_PAGE_BYTES + 1) as u64)
        .read_to_end(&mut decoded)
        .map_err(|error| FormatError::new("PVRZ", format!("zlib decode: {error}")))?;
    if decoded.len() != expected {
        return Err(FormatError::new(
            "PVRZ",
            format!(
                "decoded {} bytes; header declares {expected}",
                decoded.len()
            ),
        ));
    }
    decode_pvr(&decoded)
}

fn decode_pvr(bytes: &[u8]) -> Result<RgbaImage, FormatError> {
    if bytes.len() < 52 || &bytes[0..4] != b"PVR\x03" {
        return Err(FormatError::new("PVR V3", "invalid header"));
    }
    let pixel_format = u64::from_le_bytes(
        bytes[8..16]
            .try_into()
            .map_err(|_| FormatError::new("PVR V3", "invalid pixel format"))?,
    );
    if pixel_format != 7 {
        return Err(FormatError::new(
            "PVR V3",
            format!("pixel format {pixel_format} is not DXT1"),
        ));
    }
    let height = usize::try_from(read_u32(bytes, 24, "PVR V3 height")?)
        .map_err(|_| FormatError::new("PVR V3", "height does not fit usize"))?;
    let width = usize::try_from(read_u32(bytes, 28, "PVR V3 width")?)
        .map_err(|_| FormatError::new("PVR V3", "width does not fit usize"))?;
    let metadata = usize::try_from(read_u32(bytes, 48, "PVR V3 metadata")?)
        .map_err(|_| FormatError::new("PVR V3", "metadata size does not fit usize"))?;
    let data_offset = 52_usize
        .checked_add(metadata)
        .ok_or_else(|| FormatError::new("PVR V3", "metadata offset overflow"))?;
    let block_width = width.div_ceil(4);
    let block_height = height.div_ceil(4);
    let data_size = block_width
        .checked_mul(block_height)
        .and_then(|count| count.checked_mul(8))
        .ok_or_else(|| FormatError::new("PVR V3", "DXT1 data size overflow"))?;
    let data = bytes
        .get(data_offset..data_offset + data_size)
        .ok_or_else(|| FormatError::bounds("PVR V3 DXT1", data_offset, data_size, bytes.len()))?;
    let pixel_len = width
        .checked_mul(height)
        .and_then(|count| count.checked_mul(4))
        .ok_or_else(|| FormatError::new("PVR V3", "pixel allocation overflow"))?;
    let mut pixels = vec![0_u8; pixel_len];
    for block_y in 0..block_height {
        for block_x in 0..block_width {
            let block_index = (block_y * block_width + block_x) * 8;
            decode_dxt1_block(
                &data[block_index..block_index + 8],
                &mut pixels,
                width,
                height,
                block_x * 4,
                block_y * 4,
            );
        }
    }
    Ok(RgbaImage {
        width: u32::try_from(width).map_err(|_| FormatError::new("PVR V3", "width exceeds u32"))?,
        height: u32::try_from(height)
            .map_err(|_| FormatError::new("PVR V3", "height exceeds u32"))?,
        pixels,
    })
}

fn read_u32(bytes: &[u8], offset: usize, context: &'static str) -> Result<u32, FormatError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| FormatError::bounds(context, offset, 4, bytes.len()))?;
    Ok(u32::from_le_bytes(value.try_into().map_err(|_| {
        FormatError::new(context, "invalid 32-bit field")
    })?))
}

fn decode_dxt1_block(
    block: &[u8],
    output: &mut [u8],
    width: usize,
    height: usize,
    start_x: usize,
    start_y: usize,
) {
    let first = u16::from_le_bytes([block[0], block[1]]);
    let second = u16::from_le_bytes([block[2], block[3]]);
    let first_rgba = rgb565(first);
    let second_rgba = rgb565(second);
    let mut colors = [first_rgba, second_rgba, [0; 4], [0; 4]];
    if first > second {
        colors[2] = mix(first_rgba, second_rgba, 2, 1, 3);
        colors[3] = mix(first_rgba, second_rgba, 1, 2, 3);
    } else {
        colors[2] = mix(first_rgba, second_rgba, 1, 1, 2);
        colors[3] = [0, 0, 0, 0];
    }
    let indices = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    for y in 0..4 {
        for x in 0..4 {
            let destination_x = start_x + x;
            let destination_y = start_y + y;
            if destination_x >= width || destination_y >= height {
                continue;
            }
            let shift = 2 * (y * 4 + x);
            let color = colors[((indices >> shift) & 3) as usize];
            let destination = (destination_y * width + destination_x) * 4;
            output[destination..destination + 4].copy_from_slice(&color);
        }
    }
}

fn rgb565(color: u16) -> [u8; 4] {
    let red = ((color >> 11) & 0x1f) as u8;
    let green = ((color >> 5) & 0x3f) as u8;
    let blue = (color & 0x1f) as u8;
    [
        (red << 3) | (red >> 2),
        (green << 2) | (green >> 4),
        (blue << 3) | (blue >> 2),
        255,
    ]
}

fn mix(first: [u8; 4], second: [u8; 4], a: u16, b: u16, divisor: u16) -> [u8; 4] {
    let channel = |index| {
        let value = (u16::from(first[index]) * a + u16::from(second[index]) * b) / divisor;
        u8::try_from(value).expect("weighted u8 average fits u8")
    };
    [channel(0), channel(1), channel(2), 255]
}

fn copy_tile(
    output: &mut [u8],
    output_width: usize,
    cell: usize,
    cells_wide: usize,
    page: &RgbaImage,
    source_x: usize,
    source_y: usize,
) -> Result<(), FormatError> {
    let page_width = usize::try_from(page.width)
        .map_err(|_| FormatError::new("PVRZ", "page width does not fit usize"))?;
    let page_height = usize::try_from(page.height)
        .map_err(|_| FormatError::new("PVRZ", "page height does not fit usize"))?;
    if source_x + TILE_DIMENSION > page_width || source_y + TILE_DIMENSION > page_height {
        return Err(FormatError::new(
            "PVRZ",
            format!("tile region ({source_x}, {source_y}) exceeds {page_width}x{page_height} page"),
        ));
    }
    let destination_x = (cell % cells_wide) * TILE_DIMENSION;
    let destination_y = (cell / cells_wide) * TILE_DIMENSION;
    for row in 0..TILE_DIMENSION {
        let source = ((source_y + row) * page_width + source_x) * 4;
        let destination = ((destination_y + row) * output_width + destination_x) * 4;
        output[destination..destination + TILE_DIMENSION * 4]
            .copy_from_slice(&page.pixels[source..source + TILE_DIMENSION * 4]);
    }
    Ok(())
}

fn decode_tile(bytes: &[u8], index: usize) -> Result<Vec<u8>, FormatError> {
    let offset = index
        .checked_mul(PALETTE_TILE_SIZE)
        .ok_or_else(|| FormatError::new("TIS V1", "tile offset overflow"))?;
    let end = offset
        .checked_add(PALETTE_TILE_SIZE)
        .ok_or_else(|| FormatError::new("TIS V1", "tile end overflow"))?;
    let tile = bytes.get(offset..end).ok_or_else(|| {
        FormatError::bounds("TIS V1 tile", offset, PALETTE_TILE_SIZE, bytes.len())
    })?;
    let mut rgba = vec![0_u8; PIXEL_COUNT * 4];
    for (pixel, palette_index) in tile[PALETTE_SIZE..].iter().copied().enumerate() {
        let palette = usize::from(palette_index) * 4;
        let output = pixel * 4;
        rgba[output] = tile[palette + 2];
        rgba[output + 1] = tile[palette + 1];
        rgba[output + 2] = tile[palette];
        rgba[output + 3] = 255;
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use openbg_domain::ResRef;

    use crate::{BaseOverlay, ResourceData, Wed};

    use super::{compose_base_layer, PALETTE_TILE_SIZE};

    #[test]
    fn composes_a_palette_tile_with_bgra_channel_order() {
        let mut tile = vec![0_u8; PALETTE_TILE_SIZE];
        tile[4..8].copy_from_slice(&[30, 20, 10, 0]);
        tile[1024..].fill(1);
        let wed = Wed {
            base: BaseOverlay {
                width: 1,
                height: 1,
                tileset: ResRef::new("TEST").expect("valid resref"),
                tile_indices: vec![0],
            },
        };
        let image = compose_base_layer(
            &wed,
            ResourceData::Tileset {
                bytes: &tile,
                tile_count: 1,
                tile_size: u32::try_from(PALETTE_TILE_SIZE).expect("test tile size fits u32"),
            },
        )
        .expect("image composes");
        assert_eq!(image.width, 64);
        assert_eq!(&image.pixels[..4], &[10, 20, 30, 255]);
    }
}
