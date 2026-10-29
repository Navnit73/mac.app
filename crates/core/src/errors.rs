//! Application error type, serializable across the IPC boundary.

use serde::Serialize;
use ts_rs::TS;

/// Error returned by every command. Never panics across IPC.
#[derive(Debug, thiserror::Error, Serialize, TS)]
#[serde(tag = "kind", content = "message")]
#[ts(export)]
pub enum AppError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("i/o error: {0}")]
    Io(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => AppError::NotFound(e.to_string()),
            std::io::ErrorKind::PermissionDenied => AppError::PermissionDenied(e.to_string()),
            _ => AppError::Io(e.to_string()),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_kind_and_message() {
        let v = serde_json::to_value(AppError::NotFound("x".into())).unwrap();
        assert_eq!(v["kind"], "NotFound");
        assert_eq!(v["message"], "x");
    }

    #[test]
    fn io_errors_map_to_kinds() {
        let e: AppError = std::io::Error::from(std::io::ErrorKind::PermissionDenied).into();
        assert!(matches!(e, AppError::PermissionDenied(_)));
    }
}
