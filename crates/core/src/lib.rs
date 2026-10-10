//! Shared logic for the GMod Workshop collection builder.
//!
//! Compiles for native targets (desktop app) and `wasm32-unknown-unknown`
//! (browser extension), so nothing here may assume a filesystem or a UI.

pub mod analysis;
pub mod changes;
pub mod community;
pub mod discover;
pub mod export;
pub mod gma;
pub mod handoff;
#[cfg(feature = "client")]
pub mod http;
pub mod model;
#[cfg(feature = "client")]
pub mod steam;
pub mod updates;

pub use model::{parse_workshop_id, Addon, AddonStatus, CollectionDraft, Kind, Snap, GMOD_APPID};
#[cfg(feature = "client")]
pub use steam::{Error, SearchPage, SearchQuery, Sort, SteamClient};
