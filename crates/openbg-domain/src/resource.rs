use std::error::Error;
use std::fmt;
use std::str::FromStr;

const MAX_RESREF_LEN: usize = 8;

/// A canonical Infinity Engine resource reference.
///
/// Resource references are one to eight printable ASCII bytes. `OpenBG` stores
/// them in uppercase because lookup is case-insensitive. Path separators and a
/// colon are rejected so this identifier cannot accidentally become a path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResRef(String);

impl ResRef {
    /// Creates a canonical resource reference.
    ///
    /// # Errors
    ///
    /// Returns [`ResRefError`] when the value is empty, longer than eight bytes,
    /// non-ASCII, contains a control byte, or contains a path separator/colon.
    pub fn new(value: impl AsRef<str>) -> Result<Self, ResRefError> {
        let value = value.as_ref();
        let length = value.len();

        if length == 0 {
            return Err(ResRefError::Empty);
        }
        if length > MAX_RESREF_LEN {
            return Err(ResRefError::TooLong { length });
        }

        for (index, byte) in value.bytes().enumerate() {
            if !byte.is_ascii() || byte.is_ascii_control() || matches!(byte, b'/' | b'\\' | b':') {
                return Err(ResRefError::InvalidByte { index, byte });
            }
        }

        Ok(Self(value.to_ascii_uppercase()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ResRef {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ResRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ResRef {
    type Err = ResRefError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResRefError {
    Empty,
    TooLong { length: usize },
    InvalidByte { index: usize, byte: u8 },
}

impl fmt::Display for ResRefError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("a resource reference cannot be empty"),
            Self::TooLong { length } => write!(
                formatter,
                "a resource reference is {length} bytes; the maximum is {MAX_RESREF_LEN}"
            ),
            Self::InvalidByte { index, byte } => write!(
                formatter,
                "resource reference contains invalid byte 0x{byte:02X} at offset {index}"
            ),
        }
    }
}

impl Error for ResRefError {}

/// A resource's semantic type, independent of its storage location.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResourceKind {
    TwoDa,
    Are,
    Bam,
    Bcs,
    Bif,
    Cre,
    Dlg,
    Eff,
    Ids,
    Itm,
    Key,
    Mos,
    Spl,
    Tis,
    Tlk,
    Wed,
    Wmp,
    /// A type code preserved from an index that `OpenBG` does not understand yet.
    Unknown(u16),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceId {
    pub resref: ResRef,
    pub kind: ResourceKind,
}

impl ResourceId {
    #[must_use]
    pub const fn new(resref: ResRef, kind: ResourceKind) -> Self {
        Self { resref, kind }
    }
}

#[cfg(test)]
mod tests {
    use super::{ResRef, ResRefError};

    #[test]
    fn resref_is_canonicalized_for_case_insensitive_lookup() {
        let reference = ResRef::new("ar0100").expect("valid reference");
        assert_eq!(reference.as_str(), "AR0100");
        assert_eq!(reference, ResRef::new("AR0100").expect("valid reference"));
    }

    #[test]
    fn resref_rejects_invalid_boundaries_and_paths() {
        assert_eq!(ResRef::new(""), Err(ResRefError::Empty));
        assert_eq!(
            ResRef::new("NINECHARS"),
            Err(ResRefError::TooLong { length: 9 })
        );
        assert!(matches!(
            ResRef::new("../AREA"),
            Err(ResRefError::InvalidByte { .. })
        ));
    }
}
