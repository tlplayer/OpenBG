use openbg_domain::ResRef;

use crate::reader::Reader;
use crate::FormatError;

const HEADER_SIZE: usize = 0x9c;
const SALE_ITEM_SIZE: usize = 0x1c;
const MAX_RECORDS: usize = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoItem {
    pub resource: ResRef,
    pub expiration: u16,
    pub charges: [u16; 3],
    pub flags: u32,
    pub stock: u32,
    pub infinite: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sto {
    pub store_type: u32,
    pub name: u32,
    pub flags: u32,
    pub sell_markup: u32,
    pub buy_markup: u32,
    pub depreciation: u32,
    pub capacity: u16,
    pub purchased_item_types: Vec<u32>,
    pub items: Vec<StoItem>,
}

impl Sto {
    /// Parses the transaction-facing subset of `STOR V1.0`.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for unsupported/truncated headers, excessive or
    /// out-of-bounds tables, and malformed item resource references.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let reader = Reader::new(bytes, "STOR V1.0");
        reader.slice(0, HEADER_SIZE)?;
        reader.expect(0, b"STOR")?;
        reader.expect(4, b"V1.0")?;

        let purchased_offset = reader.usize32(0x2c)?;
        let purchased_count = bounded(reader.usize32(0x30)?, "purchased item type")?;
        reader.records(purchased_offset, purchased_count, 4)?;
        let mut purchased_item_types = Vec::with_capacity(purchased_count);
        for index in 0..purchased_count {
            purchased_item_types.push(reader.u32(purchased_offset + index * 4)?);
        }

        let item_offset = reader.usize32(0x34)?;
        let item_count = bounded(reader.usize32(0x38)?, "sale item")?;
        reader.records(item_offset, item_count, SALE_ITEM_SIZE)?;
        let mut items = Vec::with_capacity(item_count);
        for index in 0..item_count {
            let offset = item_offset + index * SALE_ITEM_SIZE;
            items.push(StoItem {
                resource: required_resref(&reader, offset, index)?,
                expiration: reader.u16(offset + 8)?,
                charges: [
                    reader.u16(offset + 0x0a)?,
                    reader.u16(offset + 0x0c)?,
                    reader.u16(offset + 0x0e)?,
                ],
                flags: reader.u32(offset + 0x10)?,
                stock: reader.u32(offset + 0x14)?,
                infinite: reader.u32(offset + 0x18)? != 0,
            });
        }

        Ok(Self {
            store_type: reader.u32(0x08)?,
            name: reader.u32(0x0c)?,
            flags: reader.u32(0x10)?,
            sell_markup: reader.u32(0x14)?,
            buy_markup: reader.u32(0x18)?,
            depreciation: reader.u32(0x1c)?,
            capacity: reader.u16(0x22)?,
            purchased_item_types,
            items,
        })
    }
}

fn bounded(count: usize, label: &str) -> Result<usize, FormatError> {
    if count > MAX_RECORDS {
        Err(FormatError::new(
            "STOR V1.0",
            format!("{label} count {count} exceeds limit {MAX_RECORDS}"),
        ))
    } else {
        Ok(count)
    }
}

fn required_resref(
    reader: &Reader<'_>,
    offset: usize,
    index: usize,
) -> Result<ResRef, FormatError> {
    let raw = reader.array::<8>(offset)?;
    let length = raw.iter().position(|byte| *byte == 0).unwrap_or(8);
    let value = std::str::from_utf8(&raw[..length])
        .map_err(|_| FormatError::new("STOR V1.0", "item resref is not ASCII"))?;
    if value.is_empty() {
        return Err(FormatError::new(
            "STOR V1.0",
            format!("sale item {index} has an empty resref"),
        ));
    }
    ResRef::new(value).map_err(|error| FormatError::new("STOR V1.0", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::Sto;

    #[test]
    fn parses_store_prices_categories_and_stock() {
        let mut bytes = vec![0_u8; 0xbc];
        bytes[0..8].copy_from_slice(b"STORV1.0");
        bytes[0x0c..0x10].copy_from_slice(&42_u32.to_le_bytes());
        bytes[0x10..0x14].copy_from_slice(&3_u32.to_le_bytes());
        bytes[0x14..0x18].copy_from_slice(&150_u32.to_le_bytes());
        bytes[0x18..0x1c].copy_from_slice(&50_u32.to_le_bytes());
        bytes[0x2c..0x30].copy_from_slice(&0x9c_u32.to_le_bytes());
        bytes[0x30..0x34].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0x34..0x38].copy_from_slice(&0xa0_u32.to_le_bytes());
        bytes[0x38..0x3c].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0x9c..0xa0].copy_from_slice(&2_u32.to_le_bytes());
        bytes[0xa0..0xa8].copy_from_slice(b"PLAT01\0\0");
        bytes[0xaa..0xac].copy_from_slice(&1_u16.to_le_bytes());
        bytes[0xb0..0xb4].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0xb4..0xb8].copy_from_slice(&3_u32.to_le_bytes());

        let store = Sto::parse(&bytes).expect("valid synthetic store");
        assert_eq!(store.sell_markup, 150);
        assert_eq!(store.buy_markup, 50);
        assert_eq!(store.purchased_item_types, [2]);
        assert_eq!(store.items[0].resource.as_str(), "PLAT01");
        assert_eq!(store.items[0].stock, 3);
    }
}
