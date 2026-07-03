use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatError {
    context: &'static str,
    detail: String,
}

impl FormatError {
    #[must_use]
    pub fn new(context: &'static str, detail: impl Into<String>) -> Self {
        Self {
            context,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub fn bounds(context: &'static str, offset: usize, size: usize, length: usize) -> Self {
        Self::new(
            context,
            format!(
                "range {offset:#x}..{:#x} exceeds {length:#x} bytes",
                offset.saturating_add(size)
            ),
        )
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.context, self.detail)
    }
}

impl Error for FormatError {}
