use crate::FormatError;

const MAX_ENTRIES: usize = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdsEntry {
    pub value: i64,
    pub symbol: String,
}

impl IdsEntry {
    #[must_use]
    pub fn name(&self) -> &str {
        self.symbol
            .split_once('(')
            .map_or(self.symbol.as_str(), |(name, _)| name)
    }
}

/// A numeric-to-symbol mapping used by Infinity Engine scripts and rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ids {
    pub entries: Vec<IdsEntry>,
}

impl Ids {
    /// Parses an optional `IDS V1.0` header followed by numeric symbol entries.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for invalid UTF-8/ASCII, malformed headers or
    /// values, missing symbols, or excessive entry counts.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| FormatError::new("IDS V1.0", "table is not UTF-8/ASCII"))?;
        let mut entries = Vec::new();
        let mut saw_content = false;
        let mut declared_count = None;
        for (line_index, raw_line) in text.lines().enumerate() {
            let line = raw_line
                .split_once("//")
                .map_or(raw_line, |(content, _)| content)
                .trim();
            if line.is_empty() {
                continue;
            }
            if !saw_content && line.to_ascii_uppercase().starts_with("IDS") {
                let mut fields = line.split_ascii_whitespace();
                if fields
                    .next()
                    .is_none_or(|value| !value.eq_ignore_ascii_case("IDS"))
                    || fields
                        .next()
                        .is_none_or(|value| !value.eq_ignore_ascii_case("V1.0"))
                    || fields.next().is_some()
                {
                    return Err(FormatError::new(
                        "IDS V1.0",
                        format!("invalid header on line {}", line_index + 1),
                    ));
                }
                saw_content = true;
                continue;
            }
            if !saw_content && !line.chars().any(char::is_whitespace) {
                if let Ok(count) = line.parse::<usize>() {
                    declared_count = Some(count);
                    saw_content = true;
                    continue;
                }
            }
            saw_content = true;
            if entries.len() == MAX_ENTRIES {
                return Err(FormatError::new(
                    "IDS V1.0",
                    format!("entry count exceeds limit {MAX_ENTRIES}"),
                ));
            }
            let split = line.find(char::is_whitespace).ok_or_else(|| {
                FormatError::new(
                    "IDS V1.0",
                    format!("line {} entry `{line}` is missing a symbol", line_index + 1),
                )
            })?;
            let value = parse_number(&line[..split]).map_err(|detail| {
                FormatError::new("IDS V1.0", format!("line {} has {detail}", line_index + 1))
            })?;
            let symbol = line[split..].trim();
            if symbol.is_empty() {
                return Err(FormatError::new(
                    "IDS V1.0",
                    format!("line {} is missing a symbol", line_index + 1),
                ));
            }
            entries.push(IdsEntry {
                value,
                symbol: symbol.to_owned(),
            });
        }
        if declared_count.is_some_and(|count| entries.len() < count) {
            return Err(FormatError::new(
                "IDS V1.0",
                format!(
                    "table declares {} entries but contains {}",
                    declared_count.expect("checked as some"),
                    entries.len()
                ),
            ));
        }
        Ok(Self { entries })
    }

    pub fn symbols(&self, value: i64) -> impl Iterator<Item = &IdsEntry> {
        self.entries
            .iter()
            .filter(move |entry| entry.value == value)
    }

    #[must_use]
    pub fn symbol(&self, value: i64) -> Option<&IdsEntry> {
        self.symbols(value).next()
    }

    #[must_use]
    pub fn value(&self, symbol: &str) -> Option<i64> {
        self.entries
            .iter()
            .find(|entry| {
                entry.symbol.eq_ignore_ascii_case(symbol)
                    || entry.name().eq_ignore_ascii_case(symbol)
            })
            .map(|entry| entry.value)
    }
}

fn parse_number(value: &str) -> Result<i64, &'static str> {
    let (negative, value) = value
        .strip_prefix('-')
        .map_or((false, value), |value| (true, value));
    let (radix, digits) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map_or((10, value), |digits| (16, digits));
    let parsed = i64::from_str_radix(digits, radix).map_err(|_| "an invalid numeric value")?;
    if negative {
        parsed
            .checked_neg()
            .ok_or("a numeric value outside the i64 range")
    } else {
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::Ids;

    #[test]
    fn parses_decimal_hex_aliases_and_signatures() {
        let ids = Ids::parse(
            b"IDS V1.0\n0 NoAction()\n0 NONE\n0x401D Specifics(O:Object*,I:Specifics*) // note\n-1 INVALID\n",
        )
        .expect("valid synthetic IDS");

        assert_eq!(ids.symbols(0).count(), 2);
        assert_eq!(ids.value("Specifics"), Some(0x401d));
        assert_eq!(
            ids.symbol(0x401d).map(|entry| entry.name()),
            Some("Specifics")
        );
        assert_eq!(ids.value("invalid"), Some(-1));
    }

    #[test]
    fn accepts_headerless_tables_and_rejects_bad_entries() {
        assert_eq!(
            Ids::parse(b"1 ONE\n")
                .expect("header optional")
                .value("ONE"),
            Some(1)
        );
        assert!(Ids::parse(b"IDS V2.0\n").is_err());
        assert!(Ids::parse(b"wat SYMBOL\n").is_err());
        assert!(Ids::parse(b"1\n").is_err());
    }
}
