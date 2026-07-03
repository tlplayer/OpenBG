use std::io::{Read, Seek, SeekFrom};

use crate::reader::Reader;
use crate::FormatError;

const FILE_RECORD_SIZE: usize = 16;
const TILESET_RECORD_SIZE: usize = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceData<'a> {
    File(&'a [u8]),
    Tileset {
        bytes: &'a [u8],
        tile_count: u32,
        tile_size: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnedResourceData {
    File(Vec<u8>),
    Tileset {
        bytes: Vec<u8>,
        tile_count: u32,
        tile_size: u32,
    },
}

impl OwnedResourceData {
    #[must_use]
    pub fn as_borrowed(&self) -> ResourceData<'_> {
        match self {
            Self::File(bytes) => ResourceData::File(bytes),
            Self::Tileset {
                bytes,
                tile_count,
                tile_size,
            } => ResourceData::Tileset {
                bytes,
                tile_count: *tile_count,
                tile_size: *tile_size,
            },
        }
    }
}

/// A seek-based BIFF reader that avoids loading an entire archive.
pub struct BifReader<R> {
    source: R,
    file_count: u32,
    tileset_count: u32,
    table_offset: u64,
}

impl<R: Read + Seek> BifReader<R> {
    /// Opens and validates a `BIFF V1` stream.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] if the header cannot be read or is unsupported.
    pub fn new(mut source: R) -> Result<Self, FormatError> {
        let mut header = [0_u8; 20];
        source
            .read_exact(&mut header)
            .map_err(|error| FormatError::new("BIFF V1", format!("read header: {error}")))?;
        let reader = Reader::new(&header, "BIFF V1");
        reader.expect(0, b"BIFF")?;
        reader.expect(4, b"V1  ")?;
        Ok(Self {
            source,
            file_count: reader.u32(8)?,
            tileset_count: reader.u32(12)?,
            table_offset: u64::from(reader.u32(16)?),
        })
    }

    /// Reads only the selected resource payload from the archive.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] if the directory or payload cannot be read, the
    /// locator is absent, or the selected payload exceeds the safety limit.
    pub fn resource(
        &mut self,
        locator: u32,
        is_tileset: bool,
    ) -> Result<OwnedResourceData, FormatError> {
        if is_tileset {
            self.read_tileset(locator)
        } else {
            self.read_file(locator)
        }
    }

    fn read_file(&mut self, locator: u32) -> Result<OwnedResourceData, FormatError> {
        self.source
            .seek(SeekFrom::Start(self.table_offset))
            .map_err(|error| FormatError::new("BIFF V1", format!("seek file table: {error}")))?;
        let wanted = locator & 0x3fff;
        for _ in 0..self.file_count {
            let record = read_record::<16>(&mut self.source, "BIFF V1 file table")?;
            if little_u32(&record, 0) & 0x3fff == wanted {
                let offset = u64::from(little_u32(&record, 4));
                let size = little_u32(&record, 8);
                return read_payload(&mut self.source, offset, size).map(OwnedResourceData::File);
            }
        }
        Err(FormatError::new(
            "BIFF V1",
            format!("file locator {wanted:#x} was not found"),
        ))
    }

    fn read_tileset(&mut self, locator: u32) -> Result<OwnedResourceData, FormatError> {
        let table_offset = self
            .table_offset
            .checked_add(u64::from(self.file_count) * FILE_RECORD_SIZE as u64)
            .ok_or_else(|| FormatError::new("BIFF V1", "tileset table offset overflow"))?;
        self.source
            .seek(SeekFrom::Start(table_offset))
            .map_err(|error| FormatError::new("BIFF V1", format!("seek tileset table: {error}")))?;
        let wanted = (locator >> 14) & 0x3f;
        for _ in 0..self.tileset_count {
            let record = read_record::<20>(&mut self.source, "BIFF V1 tileset table")?;
            if (little_u32(&record, 0) >> 14) & 0x3f == wanted {
                let offset = u64::from(little_u32(&record, 4));
                let tile_count = little_u32(&record, 8);
                let tile_size = little_u32(&record, 12);
                let size = tile_count
                    .checked_mul(tile_size)
                    .ok_or_else(|| FormatError::new("BIFF V1", "tileset payload size overflow"))?;
                let bytes = read_payload(&mut self.source, offset, size)?;
                return Ok(OwnedResourceData::Tileset {
                    bytes,
                    tile_count,
                    tile_size,
                });
            }
        }
        Err(FormatError::new(
            "BIFF V1",
            format!("tileset locator {wanted:#x} was not found"),
        ))
    }
}

fn read_record<const N: usize>(
    source: &mut impl Read,
    context: &'static str,
) -> Result<[u8; N], FormatError> {
    let mut record = [0_u8; N];
    source
        .read_exact(&mut record)
        .map_err(|error| FormatError::new(context, error.to_string()))?;
    Ok(record)
}

fn little_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_payload(
    source: &mut (impl Read + Seek),
    offset: u64,
    size: u32,
) -> Result<Vec<u8>, FormatError> {
    const MAX_RESOURCE_SIZE: usize = 512 * 1024 * 1024;
    let size = usize::try_from(size)
        .map_err(|_| FormatError::new("BIFF V1", "resource size does not fit usize"))?;
    if size > MAX_RESOURCE_SIZE {
        return Err(FormatError::new(
            "BIFF V1",
            format!("resource size {size} exceeds safety limit {MAX_RESOURCE_SIZE}"),
        ));
    }
    source
        .seek(SeekFrom::Start(offset))
        .map_err(|error| FormatError::new("BIFF V1", format!("seek payload: {error}")))?;
    let mut bytes = vec![0_u8; size];
    source
        .read_exact(&mut bytes)
        .map_err(|error| FormatError::new("BIFF V1", format!("read payload: {error}")))?;
    Ok(bytes)
}

pub struct BifArchive<'a> {
    bytes: &'a [u8],
    file_count: usize,
    tileset_count: usize,
    table_offset: usize,
}

impl<'a> BifArchive<'a> {
    /// Parses a `BIFF V1` archive directory.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for invalid headers, bounds, or table sizes.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, FormatError> {
        let reader = Reader::new(bytes, "BIFF V1");
        reader.expect(0, b"BIFF")?;
        reader.expect(4, b"V1  ")?;
        let file_count = reader.usize32(8)?;
        let tileset_count = reader.usize32(12)?;
        let table_offset = reader.usize32(16)?;
        reader.records(table_offset, file_count, FILE_RECORD_SIZE)?;
        let tileset_offset = table_offset
            .checked_add(
                file_count
                    .checked_mul(FILE_RECORD_SIZE)
                    .ok_or_else(|| FormatError::new("BIFF V1", "file table size overflow"))?,
            )
            .ok_or_else(|| FormatError::new("BIFF V1", "tileset table offset overflow"))?;
        reader.records(tileset_offset, tileset_count, TILESET_RECORD_SIZE)?;
        Ok(Self {
            bytes,
            file_count,
            tileset_count,
            table_offset,
        })
    }

    /// Resolves one KEY locator against this archive.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] when the locator is absent or its payload is out
    /// of bounds.
    pub fn resource(
        &self,
        locator: u32,
        is_tileset: bool,
    ) -> Result<ResourceData<'a>, FormatError> {
        if is_tileset {
            self.tileset(locator)
        } else {
            self.file(locator)
        }
    }

    fn file(&self, locator: u32) -> Result<ResourceData<'a>, FormatError> {
        let reader = Reader::new(self.bytes, "BIFF V1 file");
        let wanted = locator & 0x3fff;
        for index in 0..self.file_count {
            let offset = self.table_offset + index * FILE_RECORD_SIZE;
            if reader.u32(offset)? & 0x3fff == wanted {
                let data_offset = reader.usize32(offset + 4)?;
                let size = reader.usize32(offset + 8)?;
                return Ok(ResourceData::File(reader.slice(data_offset, size)?));
            }
        }
        Err(FormatError::new(
            "BIFF V1",
            format!("file locator {wanted:#x} was not found"),
        ))
    }

    fn tileset(&self, locator: u32) -> Result<ResourceData<'a>, FormatError> {
        let reader = Reader::new(self.bytes, "BIFF V1 tileset");
        let wanted = (locator >> 14) & 0x3f;
        let start = self.table_offset + self.file_count * FILE_RECORD_SIZE;
        for index in 0..self.tileset_count {
            let offset = start + index * TILESET_RECORD_SIZE;
            if (reader.u32(offset)? >> 14) & 0x3f == wanted {
                let data_offset = reader.usize32(offset + 4)?;
                let tile_count = reader.u32(offset + 8)?;
                let tile_size = reader.u32(offset + 12)?;
                let size = usize::try_from(tile_count)
                    .ok()
                    .and_then(|count| {
                        usize::try_from(tile_size)
                            .ok()
                            .and_then(|size| count.checked_mul(size))
                    })
                    .ok_or_else(|| FormatError::new("BIFF V1", "tileset size overflow"))?;
                return Ok(ResourceData::Tileset {
                    bytes: reader.slice(data_offset, size)?,
                    tile_count,
                    tile_size,
                });
            }
        }
        Err(FormatError::new(
            "BIFF V1",
            format!("tileset locator {wanted:#x} was not found"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{BifReader, OwnedResourceData};

    #[test]
    fn seek_reader_extracts_file_and_palette_tileset() {
        let mut bytes = vec![0_u8; 60 + 5120];
        bytes[0..8].copy_from_slice(b"BIFFV1  ");
        bytes[8..12].copy_from_slice(&1_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&1_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&20_u32.to_le_bytes());

        bytes[20..24].copy_from_slice(&7_u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&56_u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&4_u32.to_le_bytes());
        bytes[32..34].copy_from_slice(&0x03e9_u16.to_le_bytes());

        let tile_locator = 2_u32 << 14;
        bytes[36..40].copy_from_slice(&tile_locator.to_le_bytes());
        bytes[40..44].copy_from_slice(&60_u32.to_le_bytes());
        bytes[44..48].copy_from_slice(&1_u32.to_le_bytes());
        bytes[48..52].copy_from_slice(&5120_u32.to_le_bytes());
        bytes[52..54].copy_from_slice(&0x03eb_u16.to_le_bytes());
        bytes[56..60].copy_from_slice(b"WED!");

        let mut file_reader = BifReader::new(Cursor::new(bytes.clone())).expect("valid BIFF");
        assert_eq!(
            file_reader.resource(7, false).expect("file exists"),
            OwnedResourceData::File(b"WED!".to_vec())
        );

        let mut tile_reader = BifReader::new(Cursor::new(bytes)).expect("valid BIFF");
        let tileset = tile_reader
            .resource(tile_locator, true)
            .expect("tileset exists");
        assert!(matches!(
            tileset,
            OwnedResourceData::Tileset {
                tile_count: 1,
                tile_size: 5120,
                ..
            }
        ));
    }
}
