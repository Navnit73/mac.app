//! DuoBolt core: all filesystem logic. No UI or Tauri dependency.

pub mod activity;
pub mod apps;
pub mod cleanup;
pub mod compress;
pub mod duplicates;
pub mod errors;
pub mod find;
pub mod index;
pub mod inspect;
pub mod license;
pub mod models;
pub mod monitor;
pub mod overview;
pub mod platform;
pub mod scanner;
pub mod space;
pub mod trash;

pub use errors::{AppError, AppResult};
