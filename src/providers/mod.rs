//! ModelProvider subsystem — re-exported from `clawcrew-providers`.

pub use clawcrew_providers::*;

// Keep traits.rs as a file module so its #[cfg(test)] block compiles.
#[path = "traits.rs"]
pub mod traits;
