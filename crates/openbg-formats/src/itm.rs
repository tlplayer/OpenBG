use openbg_domain::ResRef;

use crate::reader::Reader;
use crate::FormatError;

const HEADER_SIZE: usize = 0x72;
const ABILITY_SIZE: usize = 0x38;
const MAX_ABILITIES: usize = 16_384;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItmAbility {
    pub attack_type: u8,
    pub location: u8,
    pub icon: Option<ResRef>,
    pub speed_factor: u8,
    pub thac0_bonus: i16,
    pub damage_dice_count: u8,
    pub damage_dice_sides: u8,
    pub damage_bonus: i16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Itm {
    pub unidentified_name: u32,
    pub identified_name: u32,
    pub flags: u32,
    pub item_type: u16,
    pub equipped_appearance: String,
    pub price: u32,
    pub stack_amount: u16,
    pub inventory_icon: Option<ResRef>,
    pub ground_icon: Option<ResRef>,
    pub weight: u32,
    pub unidentified_description: u32,
    pub identified_description: u32,
    pub description_icon: Option<ResRef>,
    pub enchantment: u32,
    pub abilities: Vec<ItmAbility>,
}

impl Itm {
    /// Parses the gameplay-facing header and ability subset of `ITM V1`.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for unsupported/truncated headers, malformed
    /// resource references, unsafe ability counts, or out-of-bounds tables.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let reader = Reader::new(bytes, "ITM V1");
        reader.slice(0, HEADER_SIZE)?;
        reader.expect(0, b"ITM ")?;
        reader.expect(4, b"V1  ")?;
        let ability_offset = reader.usize32(0x64)?;
        let ability_count = usize::from(reader.u16(0x68)?);
        if ability_count > MAX_ABILITIES {
            return Err(FormatError::new(
                "ITM V1",
                format!("ability count {ability_count} exceeds limit {MAX_ABILITIES}"),
            ));
        }
        reader.records(ability_offset, ability_count, ABILITY_SIZE)?;
        let mut abilities = Vec::with_capacity(ability_count);
        for index in 0..ability_count {
            let offset = ability_offset + index * ABILITY_SIZE;
            abilities.push(ItmAbility {
                attack_type: reader.slice(offset, 1)?[0],
                location: reader.slice(offset + 2, 1)?[0],
                icon: optional_resref(&reader, offset + 4)?,
                speed_factor: reader.slice(offset + 0x12, 1)?[0],
                thac0_bonus: reader.i16(offset + 0x14)?,
                damage_dice_sides: reader.slice(offset + 0x16, 1)?[0],
                damage_dice_count: reader.slice(offset + 0x18, 1)?[0],
                damage_bonus: reader.i16(offset + 0x1a)?,
            });
        }
        Ok(Self {
            unidentified_name: reader.u32(0x08)?,
            identified_name: reader.u32(0x0c)?,
            flags: reader.u32(0x18)?,
            item_type: reader.u16(0x1c)?,
            equipped_appearance: text(&reader, 0x22, 2)?,
            price: reader.u32(0x34)?,
            stack_amount: reader.u16(0x38)?,
            inventory_icon: optional_resref(&reader, 0x3a)?,
            ground_icon: optional_resref(&reader, 0x44)?,
            weight: reader.u32(0x4c)?,
            unidentified_description: reader.u32(0x50)?,
            identified_description: reader.u32(0x54)?,
            description_icon: optional_resref(&reader, 0x58)?,
            enchantment: reader.u32(0x60)?,
            abilities,
        })
    }
}

fn text(reader: &Reader<'_>, offset: usize, length: usize) -> Result<String, FormatError> {
    let raw = reader.slice(offset, length)?;
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(length);
    let value = std::str::from_utf8(&raw[..end])
        .map_err(|_| FormatError::new("ITM V1", "text field is not ASCII"))?;
    Ok(value.to_owned())
}

fn optional_resref(reader: &Reader<'_>, offset: usize) -> Result<Option<ResRef>, FormatError> {
    let raw = reader.array::<8>(offset)?;
    let length = raw.iter().position(|byte| *byte == 0).unwrap_or(8);
    if length == 0 {
        return Ok(None);
    }
    let value = std::str::from_utf8(&raw[..length])
        .map_err(|_| FormatError::new("ITM V1", "resource reference is not ASCII"))?;
    ResRef::new(value)
        .map(Some)
        .map_err(|error| FormatError::new("ITM V1", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::Itm;

    #[test]
    fn parses_item_header_and_ability() {
        let mut bytes = vec![0_u8; 0xaa];
        bytes[0..8].copy_from_slice(b"ITM V1  ");
        bytes[8..12].copy_from_slice(&10_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&11_u32.to_le_bytes());
        bytes[0x1c..0x1e].copy_from_slice(&20_u16.to_le_bytes());
        bytes[0x22..0x24].copy_from_slice(b"SW");
        bytes[0x34..0x38].copy_from_slice(&50_u32.to_le_bytes());
        bytes[0x3a..0x42].copy_from_slice(b"ICON\0\0\0\0");
        bytes[0x4c..0x50].copy_from_slice(&7_u32.to_le_bytes());
        bytes[0x64..0x68].copy_from_slice(&0x72_u32.to_le_bytes());
        bytes[0x68..0x6a].copy_from_slice(&1_u16.to_le_bytes());
        bytes[0x72] = 1;
        bytes[0x74] = 1;
        bytes[0x76..0x7e].copy_from_slice(b"ABILITY\0");
        bytes[0x84] = 4;
        bytes[0x86..0x88].copy_from_slice(&2_i16.to_le_bytes());
        bytes[0x88] = 8;
        bytes[0x8a] = 2;
        bytes[0x8c..0x8e].copy_from_slice(&3_i16.to_le_bytes());

        let item = Itm::parse(&bytes).expect("valid synthetic item");
        assert_eq!(item.item_type, 20);
        assert_eq!(item.equipped_appearance, "SW");
        assert_eq!(item.inventory_icon.expect("icon").as_str(), "ICON");
        assert_eq!(item.abilities[0].damage_dice_count, 2);
        assert_eq!(item.abilities[0].damage_dice_sides, 8);
    }
}
