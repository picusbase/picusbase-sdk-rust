/// Auth module error codes
use crate::define_error_codes;
use crate::error::{ErrorCategory, ErrorLevel};

define_error_codes! {
    module: "AUTH",
    codes: {
        // Auth Errors
        AuthInvalidCredentials => {
            category: ErrorCategory::AUTH,
            level: ErrorLevel::Error,
            sequence: 100,
            message: "Invalid credentials"
        },
        AuthTokenExpired => {
            category: ErrorCategory::AUTH,
            level: ErrorLevel::Error,
            sequence: 101,
            message: "Token expired"
        },
        AuthTokenInvalid => {
            category: ErrorCategory::AUTH,
            level: ErrorLevel::Error,
            sequence: 102,
            message: "Invalid token"
        },
        AuthSessionExpired => {
            category: ErrorCategory::AUTH,
            level: ErrorLevel::Error,
            sequence: 103,
            message: "Session expired"
        },
        AuthMfaRequired => {
            category: ErrorCategory::AUTH,
            level: ErrorLevel::Error,
            sequence: 104,
            message: "MFA verification required"
        },

        // Business Logic Errors
        BizAccountLocked => {
            category: ErrorCategory::BIZ,
            level: ErrorLevel::Error,
            sequence: 500,
            message: "Account is locked"
        },
        BizAccountDisabled => {
            category: ErrorCategory::BIZ,
            level: ErrorLevel::Error,
            sequence: 501,
            message: "Account is disabled"
        },
        BizPasswordExpired => {
            category: ErrorCategory::BIZ,
            level: ErrorLevel::Error,
            sequence: 502,
            message: "Password has expired"
        },
    }
}
