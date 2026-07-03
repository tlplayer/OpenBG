//! Stable, engine-independent types shared by `OpenBG`'s core crates.

mod id;
mod resource;
mod time;

pub use id::EntityId;
pub use resource::{ResRef, ResRefError, ResourceId, ResourceKind};
pub use time::{GameTick, TickOverflow};
