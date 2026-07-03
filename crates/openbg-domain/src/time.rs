use std::error::Error;
use std::fmt;

/// The authoritative, monotonically increasing simulation clock.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GameTick(u64);

impl GameTick {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Advances the authoritative clock by one tick.
    ///
    /// # Errors
    ///
    /// Returns [`TickOverflow`] when the counter is already [`u64::MAX`].
    pub fn next(self) -> Result<Self, TickOverflow> {
        self.0.checked_add(1).map(Self).ok_or(TickOverflow)
    }
}

impl fmt::Display for GameTick {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TickOverflow;

impl fmt::Display for TickOverflow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the game tick counter overflowed")
    }
}

impl Error for TickOverflow {}
