//! Stable, engine-independent types shared by `OpenBG`'s core crates.

mod id;
mod navigation;
mod resource;
mod time;

pub use id::EntityId;
pub use navigation::{GridError, GridPoint, NavigationGrid};
pub use resource::{ResRef, ResRefError, ResourceId, ResourceKind};
pub use time::{GameTick, TickOverflow};
