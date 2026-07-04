use openbg_domain::ResRef;

use crate::reader::Reader;
use crate::FormatError;

const ACTOR_SIZE: usize = 0x110;
const REGION_SIZE: usize = 0xc4;
const ENTRANCE_SIZE: usize = 0x68;
const ANIMATION_SIZE: usize = 0x4c;
const MAX_ACTORS: usize = 16_384;
const MAX_REGIONS: usize = 16_384;
const MAX_ENTRANCES: usize = 16_384;
const MAX_ANIMATIONS: usize = 16_384;

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
pub struct AreRegion {
    pub name: String,
    pub kind: u16,
    pub bounds: [u16; 4],
    pub destination_area: Option<ResRef>,
    pub destination_entrance: String,
    pub flags: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreEntrance {
    pub name: String,
    pub position: [u16; 2],
    pub orientation: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreAnimation {
    pub name: String,
    pub position: [u16; 2],
    pub schedule: u32,
    pub animation: ResRef,
    pub sequence: u16,
    pub frame: u16,
    pub flags: u32,
    pub height: u16,
    pub transparency: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Are {
    pub wed: ResRef,
    pub actors: Vec<AreActor>,
    pub regions: Vec<AreRegion>,
    pub entrances: Vec<AreEntrance>,
    pub animations: Vec<AreAnimation>,
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
        let region_count = usize::from(reader.u16(0x5a)?);
        let region_offset = reader.usize32(0x5c)?;
        validate_table(
            &reader,
            region_offset,
            region_count,
            REGION_SIZE,
            "region",
            MAX_REGIONS,
        )?;
        let entrance_offset = reader.usize32(0x68)?;
        let entrance_count = reader.usize32(0x6c)?;
        validate_table(
            &reader,
            entrance_offset,
            entrance_count,
            ENTRANCE_SIZE,
            "entrance",
            MAX_ENTRANCES,
        )?;
        let animation_count = reader.usize32(0xac)?;
        let animation_offset = reader.usize32(0xb0)?;
        validate_table(
            &reader,
            animation_offset,
            animation_count,
            ANIMATION_SIZE,
            "animation",
            MAX_ANIMATIONS,
        )?;

        let mut actors = Vec::with_capacity(actor_count);
        for index in 0..actor_count {
            let offset =
                actor_offset
                    .checked_add(index.checked_mul(ACTOR_SIZE).ok_or_else(|| {
                        FormatError::new("ARE V1.0", "actor record offset overflow")
                    })?)
                    .ok_or_else(|| FormatError::new("ARE V1.0", "actor record offset overflow"))?;
            actors.push(AreActor {
                name: text(&reader, offset, 32)?,
                position: [reader.u16(offset + 0x20)?, reader.u16(offset + 0x22)?],
                destination: [reader.u16(offset + 0x24)?, reader.u16(offset + 0x26)?],
                flags: reader.u32(offset + 0x28)?,
                animation_id: reader.u32(offset + 0x30)?,
                orientation: reader.u16(offset + 0x34)?,
                creature: optional_resref(&reader, offset + 0x80)?,
            });
        }

        let mut regions = Vec::with_capacity(region_count);
        for index in 0..region_count {
            let offset = region_offset + index * REGION_SIZE;
            regions.push(AreRegion {
                name: text(&reader, offset, 32)?,
                kind: reader.u16(offset + 0x20)?,
                bounds: [
                    reader.u16(offset + 0x22)?,
                    reader.u16(offset + 0x24)?,
                    reader.u16(offset + 0x26)?,
                    reader.u16(offset + 0x28)?,
                ],
                destination_area: optional_resref(&reader, offset + 0x38)?,
                destination_entrance: text(&reader, offset + 0x40, 32)?,
                flags: reader.u32(offset + 0x60)?,
            });
        }

        let mut entrances = Vec::with_capacity(entrance_count);
        for index in 0..entrance_count {
            let offset = entrance_offset + index * ENTRANCE_SIZE;
            entrances.push(AreEntrance {
                name: text(&reader, offset, 32)?,
                position: [reader.u16(offset + 0x20)?, reader.u16(offset + 0x22)?],
                orientation: reader.u16(offset + 0x24)?,
            });
        }

        let mut animations = Vec::with_capacity(animation_count);
        for index in 0..animation_count {
            let offset = animation_offset + index * ANIMATION_SIZE;
            animations.push(AreAnimation {
                name: text(&reader, offset, 32)?,
                position: [reader.u16(offset + 0x20)?, reader.u16(offset + 0x22)?],
                schedule: reader.u32(offset + 0x24)?,
                animation: required_resref(&reader, offset + 0x28, "area animation")?,
                sequence: reader.u16(offset + 0x30)?,
                frame: reader.u16(offset + 0x32)?,
                flags: reader.u32(offset + 0x34)?,
                height: reader.u16(offset + 0x38)?,
                transparency: reader.u16(offset + 0x3a)?,
            });
        }
        Ok(Self {
            wed,
            actors,
            regions,
            entrances,
            animations,
        })
    }
}

fn validate_table(
    reader: &Reader<'_>,
    offset: usize,
    count: usize,
    stride: usize,
    label: &str,
    maximum: usize,
) -> Result<(), FormatError> {
    if count > maximum {
        return Err(FormatError::new(
            "ARE V1.0",
            format!("{label} count {count} exceeds limit {maximum}"),
        ));
    }
    reader.records(offset, count, stride)
}

fn text(reader: &Reader<'_>, offset: usize, length: usize) -> Result<String, FormatError> {
    let raw = reader.slice(offset, length)?;
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(length);
    Ok(String::from_utf8_lossy(&raw[..end]).into_owned())
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
    fn parses_area_placements() {
        let mut bytes = vec![0_u8; 0x34c];
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

        bytes[0x5a..0x5c].copy_from_slice(&1_u16.to_le_bytes());
        bytes[0x5c..0x60].copy_from_slice(&0x220_u32.to_le_bytes());
        let region = 0x220;
        bytes[region..region + 4].copy_from_slice(b"Exit");
        bytes[region + 0x20..region + 0x22].copy_from_slice(&2_u16.to_le_bytes());
        for (offset, value) in [(0x22, 10_u16), (0x24, 20), (0x26, 30), (0x28, 40)] {
            bytes[region + offset..region + offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes[region + 0x38..region + 0x40].copy_from_slice(b"ARNEXT\0\0");
        bytes[region + 0x40..region + 0x44].copy_from_slice(b"Gate");

        bytes[0x68..0x6c].copy_from_slice(&0x290_u32.to_le_bytes());
        bytes[0x6c..0x70].copy_from_slice(&1_u32.to_le_bytes());
        let entrance = 0x290;
        bytes[entrance..entrance + 4].copy_from_slice(b"Gate");
        bytes[entrance + 0x20..entrance + 0x22].copy_from_slice(&55_u16.to_le_bytes());
        bytes[entrance + 0x22..entrance + 0x24].copy_from_slice(&66_u16.to_le_bytes());
        bytes[entrance + 0x24..entrance + 0x26].copy_from_slice(&8_u16.to_le_bytes());

        bytes[0xac..0xb0].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0xb0..0xb4].copy_from_slice(&0x300_u32.to_le_bytes());
        let animation = 0x300;
        bytes[animation..animation + 8].copy_from_slice(b"Fountain");
        bytes[animation + 0x20..animation + 0x22].copy_from_slice(&400_u16.to_le_bytes());
        bytes[animation + 0x22..animation + 0x24].copy_from_slice(&500_u16.to_le_bytes());
        bytes[animation + 0x24..animation + 0x28].copy_from_slice(&0x00ff_ffff_u32.to_le_bytes());
        bytes[animation + 0x28..animation + 0x30].copy_from_slice(b"FOUNTN\0\0");
        bytes[animation + 0x34..animation + 0x38].copy_from_slice(&0x1007_u32.to_le_bytes());

        let area = Are::parse(&bytes).expect("synthetic ARE is valid");
        assert_eq!(area.wed.as_str(), "ARTEST");
        assert_eq!(area.actors[0].name, "Xvart");
        assert_eq!(area.actors[0].position, [120, 240]);
        assert_eq!(area.actors[0].animation_id, 0x7000);
        assert_eq!(
            area.actors[0].creature.as_ref().map(ResRef::as_str),
            Some("XVART01")
        );
        assert_eq!(area.regions[0].bounds, [10, 20, 30, 40]);
        assert_eq!(
            area.regions[0]
                .destination_area
                .as_ref()
                .map(ResRef::as_str),
            Some("ARNEXT")
        );
        assert_eq!(area.entrances[0].name, "Gate");
        assert_eq!(area.entrances[0].position, [55, 66]);
        assert_eq!(area.entrances[0].orientation, 8);
        assert_eq!(area.animations[0].animation.as_str(), "FOUNTN");
        assert_eq!(area.animations[0].position, [400, 500]);
    }

    use openbg_domain::ResRef;
}
