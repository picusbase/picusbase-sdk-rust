/// Core system error codes
use crate::define_error_codes;
use crate::error::{ErrorCategory, ErrorLevel};

define_error_codes! {
    module: "CORE",
    codes: {
        // API Errors
        ApiInternalError => {
            category: ErrorCategory::API,
            level: ErrorLevel::Error,
            sequence: 1,
            message: "Internal server error"
        },
        ApiServiceUnavailable => {
            category: ErrorCategory::API,
            level: ErrorLevel::Error,
            sequence: 2,
            message: "Service unavailable"
        },

        // Database Errors
        DbConnectionPoolExhausted => {
            category: ErrorCategory::DB,
            level: ErrorLevel::Error,
            sequence: 400,
            message: "Database connection pool exhausted"
        },
        DbTransactionFailed => {
            category: ErrorCategory::DB,
            level: ErrorLevel::Error,
            sequence: 401,
            message: "Database transaction failed"
        },

        // Data Errors
        DataSerializationFailed => {
            category: ErrorCategory::DATA,
            level: ErrorLevel::Error,
            sequence: 200,
            message: "Data serialization failed"
        },
        DataDeserializationFailed => {
            category: ErrorCategory::DATA,
            level: ErrorLevel::Error,
            sequence: 201,
            message: "Data deserialization failed"
        },

        // Business Logic Errors
        BizConfigurationError => {
            category: ErrorCategory::BIZ,
            level: ErrorLevel::Error,
            sequence: 500,
            message: "System configuration error"
        },
        BizMaintenanceMode => {
            category: ErrorCategory::BIZ,
            level: ErrorLevel::Warning,
            sequence: 501,
            message: "System is in maintenance mode"
        },
    }
}
