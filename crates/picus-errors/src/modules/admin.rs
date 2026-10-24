/// Admin module error codes
use crate::define_error_codes;
use crate::error::{ErrorCategory, ErrorLevel};

define_error_codes! {
    module: "ADMIN",
    codes: {
        // API Errors (001-099)
        ApiRequestFailed => {
            category: ErrorCategory::API,
            level: ErrorLevel::Error,
            sequence: 1,
            message: "API request failed"
        },
        ApiResponseParseFailed => {
            category: ErrorCategory::API,
            level: ErrorLevel::Error,
            sequence: 2,
            message: "Failed to parse API response"
        },
        ApiInvalidResponse => {
            category: ErrorCategory::API,
            level: ErrorLevel::Error,
            sequence: 3,
            message: "Invalid API response"
        },
        ApiTimeout => {
            category: ErrorCategory::API,
            level: ErrorLevel::Error,
            sequence: 4,
            message: "API request timeout"
        },

        // Auth Errors (100-199)
        AuthNoToken => {
            category: ErrorCategory::AUTH,
            level: ErrorLevel::Error,
            sequence: 100,
            message: "No authentication token found"
        },
        AuthInvalidToken => {
            category: ErrorCategory::AUTH,
            level: ErrorLevel::Error,
            sequence: 101,
            message: "Invalid authentication token"
        },
        AuthExpiredToken => {
            category: ErrorCategory::AUTH,
            level: ErrorLevel::Error,
            sequence: 102,
            message: "Authentication token expired"
        },
        AuthUnauthorized => {
            category: ErrorCategory::AUTH,
            level: ErrorLevel::Error,
            sequence: 103,
            message: "Unauthorized access"
        },

        // Data Errors (200-299)
        DataValidationFailed => {
            category: ErrorCategory::DATA,
            level: ErrorLevel::Error,
            sequence: 200,
            message: "Data validation failed"
        },
        DataParseFailed => {
            category: ErrorCategory::DATA,
            level: ErrorLevel::Error,
            sequence: 201,
            message: "Failed to parse data"
        },
        DataMissingField => {
            category: ErrorCategory::DATA,
            level: ErrorLevel::Error,
            sequence: 202,
            message: "Missing required field"
        },
        DataInvalidFormat => {
            category: ErrorCategory::DATA,
            level: ErrorLevel::Error,
            sequence: 203,
            message: "Invalid data format"
        },

        // Network Errors (300-399)
        NetConnectionFailed => {
            category: ErrorCategory::NET,
            level: ErrorLevel::Error,
            sequence: 300,
            message: "Network connection failed"
        },
        NetTimeout => {
            category: ErrorCategory::NET,
            level: ErrorLevel::Error,
            sequence: 301,
            message: "Network timeout"
        },
        NetDnsFailed => {
            category: ErrorCategory::NET,
            level: ErrorLevel::Error,
            sequence: 302,
            message: "DNS resolution failed"
        },

        // Database Errors (400-499)
        DbQueryFailed => {
            category: ErrorCategory::DB,
            level: ErrorLevel::Error,
            sequence: 400,
            message: "Database query failed"
        },
        DbConnectionFailed => {
            category: ErrorCategory::DB,
            level: ErrorLevel::Error,
            sequence: 401,
            message: "Database connection failed"
        },
        DbRecordNotFound => {
            category: ErrorCategory::DB,
            level: ErrorLevel::Error,
            sequence: 402,
            message: "Record not found"
        },

        // Business Logic Errors (500-599)
        BizUserAlreadyExists => {
            category: ErrorCategory::BIZ,
            level: ErrorLevel::Error,
            sequence: 500,
            message: "User already exists"
        },
        BizInvalidOperation => {
            category: ErrorCategory::BIZ,
            level: ErrorLevel::Error,
            sequence: 501,
            message: "Invalid operation"
        },
        BizPermissionDenied => {
            category: ErrorCategory::BIZ,
            level: ErrorLevel::Error,
            sequence: 502,
            message: "Permission denied"
        },
        BizInvalidRoleAssignment => {
            category: ErrorCategory::BIZ,
            level: ErrorLevel::Error,
            sequence: 503,
            message: "Invalid role assignment"
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::ErrorCode;

    #[test]
    fn test_admin_api_error() {
        let err = ApiRequestFailed.build();
        assert_eq!(err.code, "ADMIN-API-E-001");
        assert_eq!(err.module, "ADMIN");
    }

    #[test]
    fn test_admin_auth_error() {
        let err = AuthNoToken.build_with_details("Token missing in header");
        assert_eq!(err.code, "ADMIN-AUTH-E-100");
        assert!(err.details.is_some());
    }

    #[test]
    fn test_admin_data_error() {
        let err = DataMissingField.build_with_message("Email field is required");
        assert_eq!(err.code, "ADMIN-DATA-E-202");
        assert_eq!(err.message, "Email field is required");
    }
}
