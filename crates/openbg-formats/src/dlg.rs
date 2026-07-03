use openbg_domain::ResRef;

use crate::reader::Reader;
use crate::FormatError;

const HEADER_SIZE: usize = 0x30;
const STATE_SIZE: usize = 0x10;
const TRANSITION_SIZE: usize = 0x20;
const SCRIPT_REF_SIZE: usize = 0x08;
const MAX_RECORDS: usize = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DlgState {
    pub text: u32,
    pub first_transition: u32,
    pub transition_count: u32,
    pub trigger: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DlgTransition {
    pub flags: u32,
    pub text: Option<u32>,
    pub journal_text: Option<u32>,
    pub trigger: Option<String>,
    pub action: Option<String>,
    pub next_dialogue: Option<ResRef>,
    pub next_state: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dlg {
    pub states: Vec<DlgState>,
    pub transitions: Vec<DlgTransition>,
}

impl Dlg {
    /// Parses the dialogue state machine and retains trigger/action source text.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] for malformed headers, excessive tables,
    /// out-of-bounds references, or invalid resource names/script text.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let reader = Reader::new(bytes, "DLG V1.0");
        reader.slice(0, HEADER_SIZE)?;
        reader.expect(0, b"DLG ")?;
        reader.expect(4, b"V1.0")?;

        let state_count = bounded(reader.usize32(0x08)?, "state")?;
        let state_offset = reader.usize32(0x0c)?;
        let transition_count = bounded(reader.usize32(0x10)?, "transition")?;
        let transition_offset = reader.usize32(0x14)?;
        reader.records(state_offset, state_count, STATE_SIZE)?;
        reader.records(transition_offset, transition_count, TRANSITION_SIZE)?;

        let state_triggers = scripts(
            &reader,
            reader.usize32(0x18)?,
            bounded(reader.usize32(0x1c)?, "state trigger")?,
        )?;
        let transition_triggers = scripts(
            &reader,
            reader.usize32(0x20)?,
            bounded(reader.usize32(0x24)?, "transition trigger")?,
        )?;
        let actions = scripts(
            &reader,
            reader.usize32(0x28)?,
            bounded(reader.usize32(0x2c)?, "action")?,
        )?;

        let mut states = Vec::with_capacity(state_count);
        for index in 0..state_count {
            let offset = state_offset + index * STATE_SIZE;
            let first_transition = reader.u32(offset + 0x04)?;
            let count = reader.u32(offset + 0x08)?;
            let end = first_transition
                .checked_add(count)
                .ok_or_else(|| FormatError::new("DLG V1.0", "state transition range overflow"))?;
            if usize::try_from(end)
                .ok()
                .is_none_or(|end| end > transition_count)
            {
                return Err(FormatError::new(
                    "DLG V1.0",
                    format!("state {index} references transitions outside the table"),
                ));
            }
            let trigger =
                optional_script(&state_triggers, reader.u32(offset + 0x0c)?, "state trigger")?;
            states.push(DlgState {
                text: reader.u32(offset)?,
                first_transition,
                transition_count: count,
                trigger,
            });
        }

        let mut transitions = Vec::with_capacity(transition_count);
        for index in 0..transition_count {
            let offset = transition_offset + index * TRANSITION_SIZE;
            let flags = reader.u32(offset)?;
            let terminates = flags & (1 << 3) != 0;
            transitions.push(DlgTransition {
                flags,
                text: (flags & 1 != 0)
                    .then(|| reader.u32(offset + 0x04))
                    .transpose()?,
                journal_text: (flags & (1 << 4) != 0)
                    .then(|| reader.u32(offset + 0x08))
                    .transpose()?,
                trigger: if flags & (1 << 1) != 0 {
                    optional_script(
                        &transition_triggers,
                        reader.u32(offset + 0x0c)?,
                        "transition trigger",
                    )?
                } else {
                    None
                },
                action: if flags & (1 << 2) != 0 {
                    optional_script(&actions, reader.u32(offset + 0x10)?, "action")?
                } else {
                    None
                },
                next_dialogue: if terminates {
                    None
                } else {
                    optional_resref(&reader, offset + 0x14)?
                },
                next_state: (!terminates)
                    .then(|| reader.u32(offset + 0x1c))
                    .transpose()?,
            });
        }
        Ok(Self {
            states,
            transitions,
        })
    }
}

fn bounded(count: usize, label: &str) -> Result<usize, FormatError> {
    if count > MAX_RECORDS {
        Err(FormatError::new(
            "DLG V1.0",
            format!("{label} count {count} exceeds limit {MAX_RECORDS}"),
        ))
    } else {
        Ok(count)
    }
}

fn scripts(reader: &Reader<'_>, offset: usize, count: usize) -> Result<Vec<String>, FormatError> {
    reader.records(offset, count, SCRIPT_REF_SIZE)?;
    let mut result = Vec::with_capacity(count);
    for index in 0..count {
        let record = offset + index * SCRIPT_REF_SIZE;
        let text_offset = reader.usize32(record)?;
        let length = reader.usize32(record + 4)?;
        let raw = reader.slice(text_offset, length)?;
        result.push(
            std::str::from_utf8(raw)
                .map_err(|_| FormatError::new("DLG V1.0", "script text is not UTF-8/ASCII"))?
                .to_owned(),
        );
    }
    Ok(result)
}

fn optional_script(
    scripts: &[String],
    index: u32,
    label: &str,
) -> Result<Option<String>, FormatError> {
    if index == u32::MAX {
        return Ok(None);
    }
    scripts
        .get(usize::try_from(index).map_err(|_| FormatError::new("DLG V1.0", label))?)
        .cloned()
        .map(Some)
        .ok_or_else(|| FormatError::new("DLG V1.0", format!("{label} index {index} is missing")))
}

fn optional_resref(reader: &Reader<'_>, offset: usize) -> Result<Option<ResRef>, FormatError> {
    let raw = reader.array::<8>(offset)?;
    let length = raw.iter().position(|byte| *byte == 0).unwrap_or(8);
    if length == 0 {
        return Ok(None);
    }
    let value = std::str::from_utf8(&raw[..length])
        .map_err(|_| FormatError::new("DLG V1.0", "next dialogue resref is not ASCII"))?;
    ResRef::new(value)
        .map(Some)
        .map_err(|error| FormatError::new("DLG V1.0", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::Dlg;

    #[test]
    fn parses_state_text_and_terminating_reply() {
        let mut bytes = vec![0_u8; 0x70];
        bytes[0..8].copy_from_slice(b"DLG V1.0");
        bytes[0x08..0x0c].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0x0c..0x10].copy_from_slice(&0x30_u32.to_le_bytes());
        bytes[0x10..0x14].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0x14..0x18].copy_from_slice(&0x40_u32.to_le_bytes());
        bytes[0x30..0x34].copy_from_slice(&10_u32.to_le_bytes());
        bytes[0x34..0x38].copy_from_slice(&0_u32.to_le_bytes());
        bytes[0x38..0x3c].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0x3c..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
        bytes[0x40..0x44].copy_from_slice(&9_u32.to_le_bytes());
        bytes[0x44..0x48].copy_from_slice(&11_u32.to_le_bytes());

        let dialogue = Dlg::parse(&bytes).expect("valid synthetic DLG");
        assert_eq!(dialogue.states[0].text, 10);
        assert_eq!(dialogue.transitions[0].text, Some(11));
        assert!(dialogue.transitions[0].next_state.is_none());
    }
}
