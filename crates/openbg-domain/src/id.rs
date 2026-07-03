use std::fmt;
use std::num::NonZeroU64;

/// A stable game-world identifier.
///
/// Unlike a presentation-layer entity ID, this value can safely appear in
/// commands, deterministic traces, and save data. Zero is reserved as invalid.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityId(NonZeroU64);

impl EntityId {
    /// Constructs an ID, returning `None` for the reserved value zero.
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::EntityId;

    #[test]
    fn zero_is_not_a_valid_entity_id() {
        assert_eq!(EntityId::new(0), None);
        assert_eq!(EntityId::new(42).map(EntityId::get), Some(42));
    }
}
