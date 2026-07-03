use crate::reader::Reader;
use crate::FormatError;

const HEADER_SIZE: usize = 18;
const ENTRY_SIZE: usize = 26;
const MAX_ENTRIES: usize = 2_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
struct TlkEntry {
    offset: usize,
    length: usize,
}

/// Indexed strings from an Infinity Engine `TLK V1` file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tlk {
    entries: Vec<TlkEntry>,
    strings: Vec<u8>,
}

impl Tlk {
    /// Parses and bounds-checks a complete TLK string table.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for an invalid header, excessive entry count, or
    /// a string entry outside the declared string-data section.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let reader = Reader::new(bytes, "TLK V1");
        reader.slice(0, HEADER_SIZE)?;
        reader.expect(0, b"TLK ")?;
        reader.expect(4, b"V1  ")?;
        let count = reader.usize32(0x0a)?;
        if count > MAX_ENTRIES {
            return Err(FormatError::new(
                "TLK V1",
                format!("entry count {count} exceeds limit {MAX_ENTRIES}"),
            ));
        }
        let strings_offset = reader.usize32(0x0e)?;
        reader.records(HEADER_SIZE, count, ENTRY_SIZE)?;
        let strings = reader
            .slice(strings_offset, bytes.len().saturating_sub(strings_offset))?
            .to_vec();
        let mut entries = Vec::with_capacity(count);
        for index in 0..count {
            let entry = HEADER_SIZE + index * ENTRY_SIZE;
            let offset = reader.usize32(entry + 0x12)?;
            let length = reader.usize32(entry + 0x16)?;
            offset
                .checked_add(length)
                .filter(|end| *end <= strings.len())
                .ok_or_else(|| {
                    FormatError::bounds("TLK V1 string", offset, length, strings.len())
                })?;
            entries.push(TlkEntry { offset, length });
        }
        Ok(Self { entries, strings })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Resolves a string reference, trimming an optional trailing NUL.
    #[must_use]
    pub fn text(&self, strref: u32) -> Option<String> {
        let entry = self.entries.get(usize::try_from(strref).ok()?)?;
        let raw = self
            .strings
            .get(entry.offset..entry.offset + entry.length)?;
        let raw = raw.strip_suffix(&[0]).unwrap_or(raw);
        Some(String::from_utf8_lossy(raw).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::Tlk;

    #[test]
    fn resolves_length_delimited_strings() {
        let mut bytes = vec![0_u8; 49];
        bytes[0..8].copy_from_slice(b"TLK V1  ");
        bytes[0x0a..0x0e].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0x0e..0x12].copy_from_slice(&44_u32.to_le_bytes());
        bytes[18 + 0x16..18 + 0x1a].copy_from_slice(&5_u32.to_le_bytes());
        bytes[44..49].copy_from_slice(b"Hello");

        let table = Tlk::parse(&bytes).expect("valid synthetic TLK");
        assert_eq!(table.text(0).as_deref(), Some("Hello"));
        assert_eq!(table.text(1), None);
    }
}
