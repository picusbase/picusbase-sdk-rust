/// Core error type for Picus ERP system
use serde::{Deserialize, Serialize};
use std::fmt;

/// Error categories for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// API communication errors
    API,
    /// Authentication/Authorization errors
    AUTH,
    /// Data parsing/validation errors
    DATA,
    /// Network connectivity errors
    NET,
    /// Database operation errors
    DB,
    /// Business logic errors
    BIZ,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::API => write!(f, "API"),
            Self::AUTH => write!(f, "AUTH"),
            Self::DATA => write!(f, "DATA"),
            Self::NET => write!(f, "NET"),
            Self::DB => write!(f, "DB"),
            Self::BIZ => write!(f, "BIZ"),
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorLevel {
    /// Critical error - system cannot continue
    Error,
    /// Warning - operation may have issues
    Warning,
    /// Informational message
    Info,
}

impl fmt::Display for ErrorLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error => write!(f, "E"),
            Self::Warning => write!(f, "W"),
            Self::Info => write!(f, "I"),
        }
    }
}

/// Additional context for debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    /// Request ID for tracing
    pub request_id: Option<String>,
    /// User ID who triggered the error
    pub user_id: Option<String>,
    /// Project Reference (for isolation)
    pub project_ref: Option<String>,
    /// Additional key-value metadata
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

impl Default for ErrorContext {
    fn default() -> Self {
        Self {
            request_id: None,
            user_id: None,
            project_ref: None,
            metadata: None,
        }
    }
}

/// Main error type for Picus ERP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PicusError {
    /// Error code (e.g., "ADMIN-API-E-001")
    pub code: String,
    /// Module name (e.g., "ADMIN", "CORE")
    pub module: String,
    /// Error category
    pub category: ErrorCategory,
    /// Error level
    pub level: ErrorLevel,
    /// Sequence number (001-999)
    pub sequence: u16,
    /// Human-readable message
    pub message: String,
    /// Detailed error description
    pub details: Option<String>,
    /// Additional context
    pub context: ErrorContext,
}

impl PicusError {
    /// Create a new error
    pub fn new(
        module: impl Into<String>,
        category: ErrorCategory,
        level: ErrorLevel,
        sequence: u16,
        message: impl Into<String>,
    ) -> Self {
        let module = module.into();
        let code = format!("{}-{}-{}-{:03}", module, category, level, sequence);

        Self {
            code,
            module,
            category,
            level,
            sequence,
            message: message.into(),
            details: None,
            context: ErrorContext::default(),
        }
    }

    /// Add detailed description
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Add context information
    pub fn with_context(mut self, context: ErrorContext) -> Self {
        self.context = context;
        self
    }

    /// Add request ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.context.request_id = Some(request_id.into());
        self
    }

    /// Add user ID
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.context.user_id = Some(user_id.into());
        self
    }

    /// Add project ref
    pub fn with_project_ref(mut self, project_ref: impl Into<String>) -> Self {
        self.context.project_ref = Some(project_ref.into());
        self
    }

    /// Add custom metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        if self.context.metadata.is_none() {
            self.context.metadata = Some(std::collections::HashMap::new());
        }
        if let Some(ref mut metadata) = self.context.metadata {
            metadata.insert(key.into(), value.into());
        }
        self
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| self.to_string())
    }

    /// Convert to pretty JSON string
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| self.to_string())
    }
}

impl fmt::Display for PicusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;

        if let Some(details) = &self.details {
            write!(f, " - {}", details)?;
        }

        // Add context if available
        let mut context_parts = Vec::new();
        if let Some(req_id) = &self.context.request_id {
            context_parts.push(format!("request_id={}", req_id));
        }
        if let Some(user_id) = &self.context.user_id {
            context_parts.push(format!("user_id={}", user_id));
        }
        if let Some(project_ref) = &self.context.project_ref {
            context_parts.push(format!("project_ref={}", project_ref));
        }

        if !context_parts.is_empty() {
            write!(f, " ({})", context_parts.join(", "))?;
        }

        Ok(())
    }
}

impl std::error::Error for PicusError {}

impl From<PicusError> for String {
    fn from(err: PicusError) -> String {
        err.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = PicusError::new(
            "ADMIN",
            ErrorCategory::API,
            ErrorLevel::Error,
            1,
            "API request failed",
        );

        assert_eq!(err.code, "ADMIN-API-E-001");
        assert_eq!(err.module, "ADMIN");
        assert_eq!(err.category, ErrorCategory::API);
        assert_eq!(err.level, ErrorLevel::Error);
        assert_eq!(err.sequence, 1);
    }

    #[test]
    fn test_error_with_details() {
        let err = PicusError::new(
            "CORE",
            ErrorCategory::DB,
            ErrorLevel::Error,
            5,
            "Database query failed",
        )
        .with_details("Connection timeout after 30s");

        assert!(err.details.is_some());
        assert_eq!(err.details.unwrap(), "Connection timeout after 30s");
    }

    #[test]
    fn test_error_with_context() {
        let err = PicusError::new(
            "AUTH",
            ErrorCategory::AUTH,
            ErrorLevel::Error,
            2,
            "Unauthorized access",
        )
        .with_request_id("req-123")
        .with_user_id("user-456")
        .with_project_ref("p-1234");

        assert_eq!(err.context.request_id.unwrap(), "req-123");
        assert_eq!(err.context.user_id.unwrap(), "user-456");
        assert_eq!(err.context.project_ref.unwrap(), "p-1234");
    }

    #[test]
    fn test_error_display() {
        let err = PicusError::new(
            "ADMIN",
            ErrorCategory::API,
            ErrorLevel::Error,
            1,
            "Request failed",
        )
        .with_details("Network timeout");

        let display = format!("{}", err);
        assert!(display.contains("[ADMIN-API-E-001]"));
        assert!(display.contains("Request failed"));
        assert!(display.contains("Network timeout"));
    }
}
