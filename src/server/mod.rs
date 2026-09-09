#[cfg(not(feature = "local"))]
pub mod auth;
#[cfg(feature = "local")]
#[path = "../desktop_auth.rs"]
pub mod auth;
pub mod env;
pub mod roles;
pub mod storage;
