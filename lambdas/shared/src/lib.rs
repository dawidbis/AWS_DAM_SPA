//! Wspólny kod dla wszystkich Lambd Matchday DAM.
//!
//! Crate jest jedynym źródłem prawdy dla modeli domenowych. W etapie 1 z tych
//! typów generowane są typy TypeScript dla Angulara (`ts-rs`).

pub mod status;
pub mod telemetry;

pub use status::{AssetStatus, TransitionError};
