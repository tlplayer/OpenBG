use openbg_domain::ResRef;

use crate::reader::Reader;
use crate::FormatError;

const HEADER_SIZE: usize = 0x2d4;

/// Conversation-facing fields from a `CRE V1.0` creature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cre {
    pub long_name: u32,
    pub short_name: u32,
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
        Ok(Self {
            long_name: reader.u32(0x08)?,
            short_name: reader.u32(0x0c)?,
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
        .map_err(|_| FormatError::new("CRE V1.0", "dialogue resref is not ASCII"))?;
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
        bytes[0x2cc..0x2d4].copy_from_slice(b"TESTDLG\0");

        let creature = Cre::parse(&bytes).expect("valid synthetic CRE");
        assert_eq!(creature.long_name, 123);
        assert_eq!(creature.short_name, 456);
        assert_eq!(creature.dialogue.expect("dialogue").as_str(), "TESTDLG");
    }
}
