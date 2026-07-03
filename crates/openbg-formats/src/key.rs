use openbg_domain::{ResRef, ResourceId, ResourceKind};

use crate::reader::Reader;
use crate::FormatError;

const HEADER_SIZE: usize = 24;
const BIF_RECORD_SIZE: usize = 12;
const RESOURCE_RECORD_SIZE: usize = 14;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BifRecord {
    pub path: String,
    pub expected_size: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceRecord {
    pub id: ResourceId,
    pub locator: u32,
}

impl ResourceRecord {
    #[must_use]
    pub fn bif_index(&self) -> usize {
        let bytes = (self.locator >> 20).to_le_bytes();
        usize::from(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyIndex {
    pub bifs: Vec<BifRecord>,
    pub resources: Vec<ResourceRecord>,
}

impl KeyIndex {
    /// Parses a `KEY V1` resource index.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for an unsupported header, invalid bounds,
    /// malformed paths, resource names, or impractically large tables.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let reader = Reader::new(bytes, "KEY V1");
        reader.slice(0, HEADER_SIZE)?;
        reader.expect(0, b"KEY ")?;
        reader.expect(4, b"V1  ")?;

        let bif_count = bounded_count(reader.usize32(8)?, "BIF")?;
        let resource_count = bounded_count(reader.usize32(12)?, "resource")?;
        let bif_offset = reader.usize32(16)?;
        let resource_offset = reader.usize32(20)?;
        reader.records(bif_offset, bif_count, BIF_RECORD_SIZE)?;
        reader.records(resource_offset, resource_count, RESOURCE_RECORD_SIZE)?;

        let mut bifs = Vec::with_capacity(bif_count);
        for index in 0..bif_count {
            let offset = table_offset(bif_offset, index, BIF_RECORD_SIZE, "KEY V1")?;
            let expected_size = reader.u32(offset)?;
            let name_offset = reader.usize32(offset + 4)?;
            let name_length = usize::from(reader.u16(offset + 8)?);
            let raw = reader.slice(name_offset, name_length)?;
            let raw = raw.strip_suffix(&[0]).unwrap_or(raw);
            let path = std::str::from_utf8(raw)
                .map_err(|_| FormatError::new("KEY V1", "BIF path is not UTF-8/ASCII"))?
                .replace('\\', "/");
            validate_relative_path(&path)?;
            bifs.push(BifRecord {
                path,
                expected_size,
            });
        }

        let mut resources = Vec::with_capacity(resource_count);
        for index in 0..resource_count {
            let offset = table_offset(resource_offset, index, RESOURCE_RECORD_SIZE, "KEY V1")?;
            let raw_name = reader.array::<8>(offset)?;
            let name_length = raw_name.iter().position(|byte| *byte == 0).unwrap_or(8);
            let name = std::str::from_utf8(&raw_name[..name_length])
                .map_err(|_| FormatError::new("KEY V1", "resource name is not ASCII"))?;
            let resref = ResRef::new(name)
                .map_err(|error| FormatError::new("KEY V1 resource name", error.to_string()))?;
            let resource_type = reader.u16(offset + 8)?;
            resources.push(ResourceRecord {
                id: ResourceId::new(resref, kind_from_code(resource_type)),
                locator: reader.u32(offset + 10)?,
            });
        }

        Ok(Self { bifs, resources })
    }

    #[must_use]
    pub fn find(&self, id: &ResourceId) -> Option<&ResourceRecord> {
        self.resources.iter().rev().find(|record| record.id == *id)
    }
}

fn kind_from_code(code: u16) -> ResourceKind {
    match code {
        0x03e8 => ResourceKind::Bam,
        0x03e9 => ResourceKind::Wed,
        0x03eb => ResourceKind::Tis,
        0x03ec => ResourceKind::Mos,
        0x03ed => ResourceKind::Itm,
        0x03ee => ResourceKind::Spl,
        0x03ef => ResourceKind::Bcs,
        0x03f0 => ResourceKind::Ids,
        0x03f1 => ResourceKind::Cre,
        0x03f2 => ResourceKind::Are,
        0x03f8 => ResourceKind::Eff,
        0x03f3 => ResourceKind::Dlg,
        0x03f7 => ResourceKind::TwoDa,
        0x03fe => ResourceKind::Wmp,
        0x0404 => ResourceKind::Pvrz,
        other => ResourceKind::Unknown(other),
    }
}

fn validate_relative_path(path: &str) -> Result<(), FormatError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.split('/').any(|component| component == "..")
        || path.as_bytes().get(1) == Some(&b':')
    {
        return Err(FormatError::new("KEY V1", "unsafe BIF path"));
    }
    Ok(())
}

fn bounded_count(count: usize, label: &str) -> Result<usize, FormatError> {
    const MAX_RECORDS: usize = 4_000_000;
    if count > MAX_RECORDS {
        Err(FormatError::new(
            "KEY V1",
            format!("{label} count {count} exceeds safety limit {MAX_RECORDS}"),
        ))
    } else {
        Ok(count)
    }
}

fn table_offset(
    start: usize,
    index: usize,
    stride: usize,
    context: &'static str,
) -> Result<usize, FormatError> {
    index
        .checked_mul(stride)
        .and_then(|relative| start.checked_add(relative))
        .ok_or_else(|| FormatError::new(context, "record offset overflow"))
}

#[cfg(test)]
mod tests {
    use openbg_domain::{ResRef, ResourceId, ResourceKind};

    use super::KeyIndex;

    #[test]
    fn parses_a_synthetic_key_index() {
        let mut bytes = vec![0_u8; 64];
        bytes[0..8].copy_from_slice(b"KEY V1  ");
        bytes[8..12].copy_from_slice(&1_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&1_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&24_u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&36_u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&123_u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&50_u32.to_le_bytes());
        bytes[32..34].copy_from_slice(&14_u16.to_le_bytes());
        bytes[36..44].copy_from_slice(b"ar2600\0\0");
        bytes[44..46].copy_from_slice(&0x03e9_u16.to_le_bytes());
        bytes[46..50].copy_from_slice(&7_u32.to_le_bytes());
        bytes[50..64].copy_from_slice(b"data/area.bif\0");

        let index = KeyIndex::parse(&bytes).expect("synthetic KEY is valid");
        assert_eq!(index.bifs[0].path, "data/area.bif");
        let id = ResourceId::new(
            ResRef::new("AR2600").expect("valid resref"),
            ResourceKind::Wed,
        );
        assert_eq!(index.find(&id).map(|record| record.locator), Some(7));
    }
}
