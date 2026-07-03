use openbg_domain::ResRef;

use crate::reader::Reader;
use crate::FormatError;

const ACTOR_SIZE: usize = 0x110;
const MAX_ACTORS: usize = 16_384;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreActor {
    pub name: String,
    pub position: [u16; 2],
    pub destination: [u16; 2],
    pub flags: u32,
    pub animation_id: u32,
    pub orientation: u16,
    pub creature: Option<ResRef>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Are {
    pub wed: ResRef,
    pub actors: Vec<AreActor>,
}

impl Are {
    /// Parses the actor-placement subset of an `ARE V1.0` resource.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for an invalid header, malformed resref, unsafe
    /// actor count, or an out-of-bounds actor table.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let reader = Reader::new(bytes, "ARE V1.0");
        reader.expect(0, b"AREA")?;
        reader.expect(4, b"V1.0")?;
        let wed = required_resref(&reader, 8, "area WED")?;
        let actor_offset = reader.usize32(0x54)?;
        let actor_count = usize::from(reader.u16(0x58)?);
        if actor_count > MAX_ACTORS {
            return Err(FormatError::new(
                "ARE V1.0",
                format!("actor count {actor_count} exceeds limit {MAX_ACTORS}"),
            ));
        }
        reader.records(actor_offset, actor_count, ACTOR_SIZE)?;

        let mut actors = Vec::with_capacity(actor_count);
        for index in 0..actor_count {
            let offset =
                actor_offset
                    .checked_add(index.checked_mul(ACTOR_SIZE).ok_or_else(|| {
                        FormatError::new("ARE V1.0", "actor record offset overflow")
                    })?)
                    .ok_or_else(|| FormatError::new("ARE V1.0", "actor record offset overflow"))?;
            let raw_name = reader.slice(offset, 32)?;
            let name_len = raw_name.iter().position(|byte| *byte == 0).unwrap_or(32);
            let name = String::from_utf8_lossy(&raw_name[..name_len]).into_owned();
            actors.push(AreActor {
                name,
                position: [reader.u16(offset + 0x20)?, reader.u16(offset + 0x22)?],
                destination: [reader.u16(offset + 0x24)?, reader.u16(offset + 0x26)?],
                flags: reader.u32(offset + 0x28)?,
                animation_id: reader.u32(offset + 0x30)?,
                orientation: reader.u16(offset + 0x34)?,
                creature: optional_resref(&reader, offset + 0x80)?,
            });
        }
        Ok(Self { wed, actors })
    }
}

fn required_resref(
    reader: &Reader<'_>,
    offset: usize,
    field: &'static str,
) -> Result<ResRef, FormatError> {
    optional_resref(reader, offset)?.ok_or_else(|| FormatError::new("ARE V1.0", field))
}

fn optional_resref(reader: &Reader<'_>, offset: usize) -> Result<Option<ResRef>, FormatError> {
    let raw = reader.array::<8>(offset)?;
    let length = raw.iter().position(|byte| *byte == 0).unwrap_or(8);
    if length == 0 {
        return Ok(None);
    }
    let value = std::str::from_utf8(&raw[..length])
        .map_err(|_| FormatError::new("ARE V1.0", "resref is not ASCII"))?;
    ResRef::new(value)
        .map(Some)
        .map_err(|error| FormatError::new("ARE V1.0", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::Are;

    #[test]
    fn parses_actor_placements() {
        let mut bytes = vec![0_u8; 0x100 + 0x110];
        bytes[0..8].copy_from_slice(b"AREAV1.0");
        bytes[8..16].copy_from_slice(b"ARTEST\0\0");
        bytes[0x54..0x58].copy_from_slice(&0x100_u32.to_le_bytes());
        bytes[0x58..0x5a].copy_from_slice(&1_u16.to_le_bytes());
        let actor = 0x100;
        bytes[actor..actor + 5].copy_from_slice(b"Xvart");
        bytes[actor + 0x20..actor + 0x22].copy_from_slice(&120_u16.to_le_bytes());
        bytes[actor + 0x22..actor + 0x24].copy_from_slice(&240_u16.to_le_bytes());
        bytes[actor + 0x30..actor + 0x34].copy_from_slice(&0x7000_u32.to_le_bytes());
        bytes[actor + 0x34..actor + 0x36].copy_from_slice(&6_u16.to_le_bytes());
        bytes[actor + 0x80..actor + 0x88].copy_from_slice(b"XVART01\0");

        let area = Are::parse(&bytes).expect("synthetic ARE is valid");
        assert_eq!(area.wed.as_str(), "ARTEST");
        assert_eq!(area.actors[0].name, "Xvart");
        assert_eq!(area.actors[0].position, [120, 240]);
        assert_eq!(area.actors[0].animation_id, 0x7000);
        assert_eq!(
            area.actors[0].creature.as_ref().map(ResRef::as_str),
            Some("XVART01")
        );
    }

    use openbg_domain::ResRef;
}
