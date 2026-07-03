use openbg_domain::ResRef;

use crate::reader::Reader;
use crate::FormatError;

const HEADER_SIZE: usize = 0x2d4;
const ITEM_SIZE: usize = 0x14;
const ITEM_SLOT_COUNT: usize = 38;
const ITEM_SLOT_WORDS: usize = 40;
const MAX_ITEMS: usize = 16_384;

/// The seven customizable avatar color-ramp indices stored by `CRE V1.0`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreColors {
    pub metal: u8,
    pub minor: u8,
    pub major: u8,
    pub skin: u8,
    pub leather: u8,
    pub armor: u8,
    pub hair: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreScripts {
    pub override_script: Option<ResRef>,
    pub class: Option<ResRef>,
    pub race: Option<ResRef>,
    pub general: Option<ResRef>,
    pub default: Option<ResRef>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreItem {
    pub resource: ResRef,
    pub expiration: u16,
    pub charges: [u16; 3],
    pub flags: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreInventory {
    pub slots: Vec<Option<usize>>,
    pub items: Vec<CreItem>,
    pub selected_weapon: u16,
    pub selected_weapon_ability: u16,
}

impl CreColors {
    #[must_use]
    pub const fn as_array(self) -> [u8; 7] {
        [
            self.metal,
            self.minor,
            self.major,
            self.skin,
            self.leather,
            self.armor,
            self.hair,
        ]
    }
}

/// Conversation-facing fields from a `CRE V1.0` creature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cre {
    pub long_name: u32,
    pub short_name: u32,
    pub animation_id: u32,
    pub colors: CreColors,
    pub scripts: CreScripts,
    pub inventory: Option<CreInventory>,
    pub dialogue: Option<ResRef>,
}

impl Cre {
    /// Parses the stable name and dialogue fields from a `CRE V1.0` resource.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for an unsupported/truncated header or malformed
    /// dialogue resource reference.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let reader = Reader::new(bytes, "CRE V1.0");
        reader.slice(0, HEADER_SIZE)?;
        reader.expect(0, b"CRE ")?;
        reader.expect(4, b"V1.0")?;
        let item_slots_offset = reader.usize32(0x2b8)?;
        let inventory = if item_slots_offset == 0 {
            None
        } else {
            let item_offset = reader.usize32(0x2bc)?;
            let item_count = reader.usize32(0x2c0)?;
            if item_count > MAX_ITEMS {
                return Err(FormatError::new(
                    "CRE V1.0",
                    format!("item count {item_count} exceeds limit {MAX_ITEMS}"),
                ));
            }
            reader.records(item_offset, item_count, ITEM_SIZE)?;
            reader.records(item_slots_offset, ITEM_SLOT_WORDS, 2)?;
            let mut items = Vec::with_capacity(item_count);
            for index in 0..item_count {
                let offset = item_offset + index * ITEM_SIZE;
                let resource = optional_resref(&reader, offset)?.ok_or_else(|| {
                    FormatError::new("CRE V1.0", format!("item {index} has an empty resref"))
                })?;
                items.push(CreItem {
                    resource,
                    expiration: reader.u16(offset + 8)?,
                    charges: [
                        reader.u16(offset + 0x0a)?,
                        reader.u16(offset + 0x0c)?,
                        reader.u16(offset + 0x0e)?,
                    ],
                    flags: reader.u32(offset + 0x10)?,
                });
            }
            let mut slots = Vec::with_capacity(ITEM_SLOT_COUNT);
            for slot in 0..ITEM_SLOT_COUNT {
                let value = reader.u16(item_slots_offset + slot * 2)?;
                if value == u16::MAX {
                    slots.push(None);
                } else {
                    let index = usize::from(value);
                    if index >= item_count {
                        return Err(FormatError::new(
                            "CRE V1.0",
                            format!("inventory slot {slot} references missing item {value}"),
                        ));
                    }
                    slots.push(Some(index));
                }
            }
            Some(CreInventory {
                slots,
                items,
                selected_weapon: reader.u16(item_slots_offset + ITEM_SLOT_COUNT * 2)?,
                selected_weapon_ability: reader
                    .u16(item_slots_offset + (ITEM_SLOT_COUNT + 1) * 2)?,
            })
        };
        Ok(Self {
            long_name: reader.u32(0x08)?,
            short_name: reader.u32(0x0c)?,
            animation_id: reader.u32(0x28)?,
            colors: CreColors {
                metal: reader.slice(0x2c, 1)?[0],
                minor: reader.slice(0x2d, 1)?[0],
                major: reader.slice(0x2e, 1)?[0],
                skin: reader.slice(0x2f, 1)?[0],
                leather: reader.slice(0x30, 1)?[0],
                armor: reader.slice(0x31, 1)?[0],
                hair: reader.slice(0x32, 1)?[0],
            },
            scripts: CreScripts {
                override_script: optional_resref(&reader, 0x248)?,
                class: optional_resref(&reader, 0x250)?,
                race: optional_resref(&reader, 0x258)?,
                general: optional_resref(&reader, 0x260)?,
                default: optional_resref(&reader, 0x268)?,
            },
            inventory,
            dialogue: optional_resref(&reader, 0x2cc)?,
        })
    }
}

fn optional_resref(reader: &Reader<'_>, offset: usize) -> Result<Option<ResRef>, FormatError> {
    let raw = reader.array::<8>(offset)?;
    let length = raw.iter().position(|byte| *byte == 0).unwrap_or(8);
    if length == 0 {
        return Ok(None);
    }
    let value = std::str::from_utf8(&raw[..length])
        .map_err(|_| FormatError::new("CRE V1.0", "resource reference is not ASCII"))?;
    if value.eq_ignore_ascii_case("NONE") {
        return Ok(None);
    }
    ResRef::new(value)
        .map(Some)
        .map_err(|error| FormatError::new("CRE V1.0", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::Cre;

    #[test]
    fn parses_names_and_dialogue_reference() {
        let mut bytes = vec![0_u8; 0x2d4];
        bytes[0..8].copy_from_slice(b"CRE V1.0");
        bytes[0x08..0x0c].copy_from_slice(&123_u32.to_le_bytes());
        bytes[0x0c..0x10].copy_from_slice(&456_u32.to_le_bytes());
        bytes[0x28..0x2c].copy_from_slice(&0x6210_u32.to_le_bytes());
        bytes[0x2c..0x33].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7]);
        bytes[0x248..0x250].copy_from_slice(b"OVERRIDE");
        bytes[0x268..0x270].copy_from_slice(b"DEFAULT\0");
        bytes[0x2cc..0x2d4].copy_from_slice(b"TESTDLG\0");

        let creature = Cre::parse(&bytes).expect("valid synthetic CRE");
        assert_eq!(creature.long_name, 123);
        assert_eq!(creature.short_name, 456);
        assert_eq!(creature.animation_id, 0x6210);
        assert_eq!(creature.colors.as_array(), [1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(
            creature.scripts.override_script.expect("override").as_str(),
            "OVERRIDE"
        );
        assert_eq!(
            creature.scripts.default.expect("default").as_str(),
            "DEFAULT"
        );
        assert_eq!(creature.dialogue.expect("dialogue").as_str(), "TESTDLG");
    }

    #[test]
    fn parses_inventory_items_and_slots() {
        let mut bytes = vec![0_u8; 0x338];
        bytes[0..8].copy_from_slice(b"CRE V1.0");
        bytes[0x2b8..0x2bc].copy_from_slice(&0x2e8_u32.to_le_bytes());
        bytes[0x2bc..0x2c0].copy_from_slice(&0x2d4_u32.to_le_bytes());
        bytes[0x2c0..0x2c4].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0x2d4..0x2dc].copy_from_slice(b"SW1H01\0\0");
        bytes[0x2de..0x2e0].copy_from_slice(&3_u16.to_le_bytes());
        bytes[0x2e4..0x2e8].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0x2e8..0x338].fill(0xff);
        bytes[0x2e8..0x2ea].copy_from_slice(&0_u16.to_le_bytes());
        bytes[0x334..0x336].copy_from_slice(&1000_u16.to_le_bytes());
        bytes[0x336..0x338].copy_from_slice(&0_u16.to_le_bytes());

        let creature = Cre::parse(&bytes).expect("valid inventory");
        let inventory = creature.inventory.expect("inventory");
        assert_eq!(inventory.items[0].resource.as_str(), "SW1H01");
        assert_eq!(inventory.items[0].charges[0], 3);
        assert_eq!(inventory.slots[0], Some(0));
        assert_eq!(inventory.selected_weapon, 1000);
    }
}
