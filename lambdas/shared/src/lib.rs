//! Wspólny kod dla wszystkich Lambd Matchday DAM.
//!
//! Crate jest jedynym źródłem prawdy dla modeli domenowych. Z typów API
//! generowane są typy TypeScript dla Angulara (`ts-rs`, przy `cargo test`).

pub mod assets;
pub mod auth;
pub mod catalog;
pub mod http;
pub mod multipart;
pub mod status;
pub mod telemetry;
pub mod upload;

pub use auth::{AuthError, Caller, UserGroup};
pub use status::{AssetStatus, TransitionError};
