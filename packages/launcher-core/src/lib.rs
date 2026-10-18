mod bridge_observation;
pub mod account;
pub mod edition;
pub mod client;
pub mod content;
pub mod diagnostic;
pub mod launch;
pub mod settings;
pub mod public_config;
pub mod legacy_online;
// Legacy origin certificate implementation is no longer used by public platform APIs.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub message: &'static str,
    /// Server-side cooldown/limit in seconds when the account service reported
    /// one. `None` for every error that is not a rate limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
}
pub type Result<T> = std::result::Result<T, AppError>;
pub fn error(code: &'static str, message: &'static str) -> AppError {
    AppError { code, message, retry_after: None }
}
pub fn error_with_delay(code: &'static str, message: &'static str, retry_after: Option<u64>) -> AppError {
    AppError { code, message, retry_after }
}
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub mod update;
pub mod distribution;
pub mod extensions;

pub mod generic;
pub mod generic_bridges;
pub mod cn115_bridges;
pub mod generic_target;
pub mod client_host;
pub mod controlled;
pub mod controlled_target;
pub mod controlled_exchange;
pub mod controlled_bridges;
pub mod platform_identity;
pub mod controlled_coordinator;
pub mod max_production;
