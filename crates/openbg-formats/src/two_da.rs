use crate::FormatError;

const MAX_COLUMNS: usize = 16_384;
const MAX_ROWS: usize = 1_000_000;
const MAX_CELLS: usize = 4_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TwoDaRow {
    pub label: String,
    pub values: Vec<String>,
}

/// A normalized Infinity Engine `2DA V1.0` rules table.
///
/// Every row contains one value per column. Omitted trailing cells are replaced
/// with the table's declared default value during parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TwoDa {
    pub default: String,
    pub columns: Vec<String>,
    pub rows: Vec<TwoDaRow>,
}

impl TwoDa {
    /// Parses a plain-text `2DA V1.0` resource.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for encrypted/non-ASCII input, missing header
    /// rows, excessive dimensions, or rows with more values than the declared
    /// column count.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        if bytes.starts_with(&[0xff, 0xff]) {
            return Err(FormatError::new(
                "2DA V1.0",
                "encrypted 2DA resources are unsupported",
            ));
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| FormatError::new("2DA V1.0", "table is not UTF-8/ASCII"))?;
        let mut lines = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("//"));

        let signature = tokens(
            lines
                .next()
                .ok_or_else(|| FormatError::new("2DA V1.0", "missing signature"))?,
        );
        if signature.as_slice() != ["2DA", "V1.0"] {
            return Err(FormatError::new(
                "2DA V1.0",
                "expected `2DA V1.0` signature",
            ));
        }

        let default_line = lines
            .next()
            .ok_or_else(|| FormatError::new("2DA V1.0", "missing default value"))?;
        let default_tokens = tokens(default_line);
        if default_tokens.len() != 1 {
            return Err(FormatError::new(
                "2DA V1.0",
                "default row must contain exactly one value",
            ));
        }
        let default = default_tokens[0].to_owned();

        let column_line = lines
            .next()
            .ok_or_else(|| FormatError::new("2DA V1.0", "missing column headings"))?;
        let columns = tokens(column_line)
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if columns.is_empty() {
            return Err(FormatError::new("2DA V1.0", "table has no columns"));
        }
        if columns.len() > MAX_COLUMNS {
            return Err(FormatError::new(
                "2DA V1.0",
                format!("column count {} exceeds limit {MAX_COLUMNS}", columns.len()),
            ));
        }
        let mut rows = Vec::new();
        for (line_index, line) in lines.enumerate() {
            if rows.len() == MAX_ROWS {
                return Err(FormatError::new(
                    "2DA V1.0",
                    format!("row count exceeds limit {MAX_ROWS}"),
                ));
            }
            let fields = tokens(line);
            if fields.is_empty() {
                continue;
            }
            let supplied = fields.len() - 1;
            if supplied > columns.len() {
                return Err(FormatError::new(
                    "2DA V1.0",
                    format!(
                        "data row {} has {supplied} values for {} columns",
                        line_index + 4,
                        columns.len()
                    ),
                ));
            }
            let cell_count = rows
                .len()
                .checked_add(1)
                .and_then(|count| count.checked_mul(columns.len()))
                .ok_or_else(|| FormatError::new("2DA V1.0", "cell count overflow"))?;
            if cell_count > MAX_CELLS {
                return Err(FormatError::new(
                    "2DA V1.0",
                    format!("cell count exceeds limit {MAX_CELLS}"),
                ));
            }
            let mut values = fields[1..]
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>();
            values.resize(columns.len(), default.clone());
            rows.push(TwoDaRow {
                label: fields[0].to_owned(),
                values,
            });
        }
        Ok(Self {
            default,
            columns,
            rows,
        })
    }

    #[must_use]
    pub fn row(&self, label: &str) -> Option<&TwoDaRow> {
        self.rows
            .iter()
            .find(|row| row.label.eq_ignore_ascii_case(label))
    }

    #[must_use]
    pub fn column_index(&self, label: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|column| column.eq_ignore_ascii_case(label))
    }

    #[must_use]
    pub fn get(&self, row: &str, column: &str) -> Option<&str> {
        let column = self.column_index(column)?;
        self.row(row)?.values.get(column).map(String::as_str)
    }
}

fn tokens(line: &str) -> Vec<&str> {
    line.split_ascii_whitespace().collect()
}

#[cfg(test)]
mod tests {
    use super::TwoDa;

    #[test]
    fn parses_and_default_fills_missing_cells() {
        let table = TwoDa::parse(
            b"  2DA V1.0\r\n\r\n****\r\nNAME VALUE WEIGHT\r\nA alpha\r\nB\r\nC beta 2345 123\r\n",
        )
        .expect("valid synthetic table");

        assert_eq!(table.default, "****");
        assert_eq!(table.columns, ["NAME", "VALUE", "WEIGHT"]);
        assert_eq!(table.get("a", "name"), Some("alpha"));
        assert_eq!(table.get("A", "WEIGHT"), Some("****"));
        assert_eq!(table.get("B", "VALUE"), Some("****"));
        assert_eq!(table.get("C", "WEIGHT"), Some("123"));
    }

    #[test]
    fn rejects_encrypted_and_overwide_rows() {
        assert!(TwoDa::parse(&[0xff, 0xff, 0, 0]).is_err());
        assert!(TwoDa::parse(b"2DA V1.0\n0\nA\nROW 1 2\n").is_err());
    }
}
