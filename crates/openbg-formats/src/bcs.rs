use crate::FormatError;

const MAX_CALLS: usize = 1_000_000;
const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;

/// The function identifiers referenced by a compiled BCS/BS script.
///
/// This first slice preserves the complete source and extracts calls for
/// symbolic inspection. Structured parameters and execution are added per
/// supported gameplay opcode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bcs {
    pub source: String,
    pub trigger_ids: Vec<i64>,
    pub action_ids: Vec<i64>,
    pub blocks: Vec<BcsBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BcsBlock {
    pub trigger_ids: Vec<i64>,
    pub action_ids: Vec<i64>,
}

impl Bcs {
    /// Parses the textual compiled-script envelope and extracts function IDs.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for non-UTF-8 input, a missing `SC` envelope,
    /// or excessive call counts. Closing/unknown structural markers are
    /// retained in `source` and ignored by this first call-indexing slice.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        if bytes.len() > MAX_SOURCE_BYTES {
            return Err(FormatError::new(
                "BCS",
                format!(
                    "script size {} exceeds limit {MAX_SOURCE_BYTES}",
                    bytes.len()
                ),
            ));
        }
        let source = std::str::from_utf8(bytes)
            .map_err(|_| FormatError::new("BCS", "script is not UTF-8/ASCII"))?;
        let tokens = structural_tokens(source);
        if tokens.first().map(String::as_str) != Some("SC")
            || tokens.last().map(String::as_str) != Some("SC")
        {
            return Err(FormatError::new("BCS", "missing SC script envelope"));
        }
        let mut trigger_ids = Vec::new();
        let mut action_ids = Vec::new();
        let mut blocks = Vec::new();
        let mut current_block: Option<BcsBlock> = None;
        for (index, token) in tokens.iter().enumerate() {
            match token.as_str() {
                "CR" => {
                    if let Some(block) = current_block.take() {
                        blocks.push(block);
                    } else {
                        current_block = Some(BcsBlock {
                            trigger_ids: Vec::new(),
                            action_ids: Vec::new(),
                        });
                    }
                }
                "TR" => {
                    if let Some(id) = call_id(&tokens, index) {
                        trigger_ids.push(id);
                        if let Some(block) = &mut current_block {
                            block.trigger_ids.push(id);
                        }
                    }
                }
                "AC" => {
                    if let Some(id) = call_id(&tokens, index) {
                        action_ids.push(id);
                        if let Some(block) = &mut current_block {
                            block.action_ids.push(id);
                        }
                    }
                }
                _ => {}
            }
            if trigger_ids.len().saturating_add(action_ids.len()) > MAX_CALLS {
                return Err(FormatError::new(
                    "BCS",
                    format!("call count exceeds limit {MAX_CALLS}"),
                ));
            }
        }
        Ok(Self {
            source: source.to_owned(),
            trigger_ids,
            action_ids,
            blocks,
        })
    }
}

fn call_id(tokens: &[String], marker: usize) -> Option<i64> {
    tokens.get(marker + 1)?.parse().ok()
}

fn structural_tokens(source: &str) -> Vec<String> {
    const MARKERS: [&str; 8] = ["SC", "CR", "CO", "TR", "RS", "RE", "AC", "OB"];
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'"' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            index += 1;
            while index < bytes.len() && bytes[index] != b'"' {
                index += 1;
            }
            index = index.saturating_add(1);
            continue;
        }
        if bytes[index].is_ascii_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            index += 1;
            continue;
        }
        let marker = MARKERS.iter().find(|marker| {
            bytes
                .get(index..index + marker.len())
                .is_some_and(|candidate| candidate == marker.as_bytes())
        });
        if let Some(marker) = marker {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push((*marker).to_owned());
            index += marker.len();
        } else {
            current.push(char::from(bytes[index]));
            index += 1;
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::Bcs;

    #[test]
    fn extracts_trigger_and_action_function_ids() {
        let script = Bcs::parse(b"SC\nCR\nCO\nTR\n1 0 0 0 0 \"\" \"\" OB OB\nTR\nCO\nRS\nRE\n100AC\n7OB OB OB 0 [0.0] 0 0 \"\" \"\" AC\nRE\nRS\nCR\nSC\n")
            .expect("valid synthetic script envelope");
        assert_eq!(script.trigger_ids, [1]);
        assert_eq!(script.action_ids, [7]);
        assert_eq!(script.blocks.len(), 1);
        assert_eq!(script.blocks[0].trigger_ids, [1]);
        assert_eq!(script.blocks[0].action_ids, [7]);
    }

    #[test]
    fn rejects_missing_envelope() {
        assert!(Bcs::parse(b"TR 1 TR").is_err());
    }
}
