//! A small deterministic simulation seam for `OpenBG`.
//!
//! M0 deliberately models only actors and hit points. Its purpose is to make
//! command ordering, seeded randomness, stable IDs, ticks, events, and state
//! checksums concrete before game-specific rules are added.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use openbg_domain::{EntityId, GameTick, TickOverflow};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameCommand {
    Spawn {
        id: EntityId,
        hit_points: u32,
    },
    Damage {
        target: EntityId,
        amount: u32,
    },
    Heal {
        target: EntityId,
        amount: u32,
    },
    RandomDamage {
        source: EntityId,
        target: EntityId,
        maximum: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameEvent {
    Spawned {
        id: EntityId,
        hit_points: u32,
    },
    Damaged {
        target: EntityId,
        amount: u32,
        remaining: u32,
    },
    Healed {
        target: EntityId,
        amount: u32,
        remaining: u32,
    },
    CommandRejected {
        command_index: usize,
        reason: RejectReason,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RejectReason {
    DuplicateEntity,
    MissingEntity,
    ZeroMaximum,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StateChecksum(u64);

impl StateChecksum {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for StateChecksum {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickOutput {
    pub tick: GameTick,
    pub events: Vec<GameEvent>,
    pub checksum: StateChecksum,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActorState {
    pub hit_points: u32,
}

pub struct Simulation {
    tick: GameTick,
    random: DeterministicRandom,
    actors: BTreeMap<EntityId, ActorState>,
}

impl Simulation {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            tick: GameTick::ZERO,
            random: DeterministicRandom::new(seed),
            actors: BTreeMap::new(),
        }
    }

    /// Applies an ordered command batch and advances the clock once.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError::TickOverflow`] if no later tick can be
    /// represented.
    pub fn tick(&mut self, commands: &[GameCommand]) -> Result<TickOutput, SimulationError> {
        self.tick = self.tick.next()?;
        let mut events = Vec::with_capacity(commands.len());

        for (command_index, command) in commands.iter().copied().enumerate() {
            self.apply(command_index, command, &mut events);
        }

        Ok(TickOutput {
            tick: self.tick,
            events,
            checksum: self.checksum(),
        })
    }

    #[must_use]
    pub fn actor(&self, id: EntityId) -> Option<ActorState> {
        self.actors.get(&id).copied()
    }

    #[must_use]
    pub fn checksum(&self) -> StateChecksum {
        let mut hash = StableHasher::new();
        hash.write_u64(self.tick.get());
        hash.write_u64(self.random.state());
        hash.write_u64(self.actors.len() as u64);
        for (id, actor) in &self.actors {
            hash.write_u64(id.get());
            hash.write_u32(actor.hit_points);
        }
        StateChecksum(hash.finish())
    }

    fn apply(&mut self, index: usize, command: GameCommand, events: &mut Vec<GameEvent>) {
        match command {
            GameCommand::Spawn { id, hit_points } => {
                if let std::collections::btree_map::Entry::Vacant(entry) = self.actors.entry(id) {
                    entry.insert(ActorState { hit_points });
                    events.push(GameEvent::Spawned { id, hit_points });
                } else {
                    reject(events, index, RejectReason::DuplicateEntity);
                }
            }
            GameCommand::Damage { target, amount } => {
                self.damage(index, target, amount, events);
            }
            GameCommand::Heal { target, amount } => {
                let Some(actor) = self.actors.get_mut(&target) else {
                    reject(events, index, RejectReason::MissingEntity);
                    return;
                };
                actor.hit_points = actor.hit_points.saturating_add(amount);
                events.push(GameEvent::Healed {
                    target,
                    amount,
                    remaining: actor.hit_points,
                });
            }
            GameCommand::RandomDamage {
                source: _,
                target,
                maximum,
            } => {
                if maximum == 0 {
                    reject(events, index, RejectReason::ZeroMaximum);
                } else {
                    let amount = self.random.range_inclusive(maximum);
                    self.damage(index, target, amount, events);
                }
            }
        }
    }

    fn damage(&mut self, index: usize, target: EntityId, amount: u32, events: &mut Vec<GameEvent>) {
        let Some(actor) = self.actors.get_mut(&target) else {
            reject(events, index, RejectReason::MissingEntity);
            return;
        };
        actor.hit_points = actor.hit_points.saturating_sub(amount);
        events.push(GameEvent::Damaged {
            target,
            amount,
            remaining: actor.hit_points,
        });
    }
}

fn reject(events: &mut Vec<GameEvent>, command_index: usize, reason: RejectReason) {
    events.push(GameEvent::CommandRejected {
        command_index,
        reason,
    });
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulationError {
    TickOverflow,
}

impl fmt::Display for SimulationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TickOverflow => formatter.write_str("simulation tick overflowed"),
        }
    }
}

impl Error for SimulationError {}

impl From<TickOverflow> for SimulationError {
    fn from(_: TickOverflow) -> Self {
        Self::TickOverflow
    }
}

/// `SplitMix64`: small, reproducible, and fully specified here.
struct DeterministicRandom {
    state: u64,
}

impl DeterministicRandom {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    const fn state(&self) -> u64 {
        self.state
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    fn range_inclusive(&mut self, maximum: u32) -> u32 {
        let value = (self.next_u64() % u64::from(maximum)) + 1;
        u32::try_from(value).expect("modulo by a u32 always fits in a u32")
    }
}

/// FNV-1a over explicitly encoded little-endian state fields.
struct StableHasher(u64);

impl StableHasher {
    const fn new() -> Self {
        Self(0xCBF2_9CE4_8422_2325)
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }

    const fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use openbg_domain::EntityId;

    use super::{GameCommand, Simulation};

    fn id(value: u64) -> EntityId {
        EntityId::new(value).expect("test IDs are non-zero")
    }

    fn run_replay(seed: u64) -> super::StateChecksum {
        let attacker = id(1);
        let target = id(2);
        let mut simulation = Simulation::new(seed);

        simulation
            .tick(&[
                GameCommand::Spawn {
                    id: attacker,
                    hit_points: 100,
                },
                GameCommand::Spawn {
                    id: target,
                    hit_points: 1_000_000,
                },
            ])
            .expect("initial tick succeeds");

        for tick in 1..10_000_u32 {
            let command = if tick % 17 == 0 {
                GameCommand::Heal {
                    target,
                    amount: tick % 11,
                }
            } else {
                GameCommand::RandomDamage {
                    source: attacker,
                    target,
                    maximum: 20,
                }
            };
            simulation.tick(&[command]).expect("replay tick succeeds");
        }

        simulation.checksum()
    }

    #[test]
    fn identical_seeded_ten_thousand_tick_replays_match() {
        let first = run_replay(0xBADD_CAFE);
        let second = run_replay(0xBADD_CAFE);

        assert_eq!(first, second);
    }

    #[test]
    fn seed_is_part_of_authoritative_state() {
        assert_ne!(run_replay(1), run_replay(2));
    }
}
