//! Wspólny kod dla wszystkich Lambd Matchday DAM.
//!
//! Crate jest jedynym źródłem prawdy dla modeli domenowych. W kolejnych
//! krokach z tych typów generowane są typy TypeScript dla Angulara (`ts-rs`).

pub mod assets;
pub mod auth;
pub mod http;
pub mod multipart;
pub mod status;
pub mod telemetry;
pub mod upload;

pub use auth::{AuthError, Caller, UserGroup};
pub use status::{AssetStatus, TransitionError};
