/// PicusBase SDK error type.
///
/// Format: [MODULE]-[CATEGORY]-[LEVEL]-[SEQUENCE]
///
/// Modules: ADMIN, AUTH, CORE, and the SDK's own modules (SDK, REALTIME, ...).
/// Categories: API, AUTH, DATA, NET, DB, BIZ (Business Logic)
/// Levels: E (Error), W (Warning), I (Info)
/// Sequence: 001-999
///
/// Example: ADMIN-API-E-001, AUTH-DB-E-005
pub mod codes;
pub mod error;
pub mod modules;

pub use codes::ErrorCode;
pub use error::{ErrorCategory, ErrorContext, ErrorLevel, PicusError};

// Re-export for convenience
pub type Result<T> = std::result::Result<T, PicusError>;
