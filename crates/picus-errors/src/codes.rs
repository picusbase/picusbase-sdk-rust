/// Predefined error codes for consistent error handling
use crate::error::{ErrorCategory, ErrorLevel, PicusError};

/// Error code builder trait
pub trait ErrorCode {
    fn code(&self) -> &'static str;
    fn message(&self) -> &'static str;
    fn category(&self) -> ErrorCategory;
    fn level(&self) -> ErrorLevel;
    fn sequence(&self) -> u16;
    fn module(&self) -> &'static str;

    /// Build a PicusError from this code
    fn build(&self) -> PicusError {
        PicusError::new(
            self.module(),
            self.category(),
            self.level(),
            self.sequence(),
            self.message(),
        )
    }

    /// Build with custom message
    fn build_with_message(&self, message: impl Into<String>) -> PicusError {
        PicusError::new(
            self.module(),
            self.category(),
            self.level(),
            self.sequence(),
            message,
        )
    }

    /// Build with details
    fn build_with_details(&self, details: impl Into<String>) -> PicusError {
        self.build().with_details(details)
    }
}

/// Macro to define error codes
#[macro_export]
macro_rules! define_error_codes {
    (
        module: $module:expr,
        codes: {
            $(
                $name:ident => {
                    category: $category:expr,
                    level: $level:expr,
                    sequence: $sequence:expr,
                    message: $message:expr
                }
            ),* $(,)?
        }
    ) => {
        $(
            pub struct $name;

            impl $crate::codes::ErrorCode for $name {
                fn code(&self) -> &'static str {
                    concat!($module, "-", stringify!($category), "-", stringify!($level), "-", stringify!($sequence))
                }

                fn message(&self) -> &'static str {
                    $message
                }

                fn category(&self) -> $crate::error::ErrorCategory {
                    $category
                }

                fn level(&self) -> $crate::error::ErrorLevel {
                    $level
                }

                fn sequence(&self) -> u16 {
                    $sequence
                }

                fn module(&self) -> &'static str {
                    $module
                }
            }
        )*
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{ErrorCategory, ErrorLevel};

    define_error_codes! {
        module: "TEST",
        codes: {
            TestApiError => {
                category: ErrorCategory::API,
                level: ErrorLevel::Error,
                sequence: 1,
                message: "Test API error"
            },
            TestAuthError => {
                category: ErrorCategory::AUTH,
                level: ErrorLevel::Error,
                sequence: 2,
                message: "Test auth error"
            },
        }
    }

    #[test]
    fn test_error_code_build() {
        let err = TestApiError.build();
        assert_eq!(err.module, "TEST");
        assert_eq!(err.category, ErrorCategory::API);
        assert_eq!(err.message, "Test API error");
    }

    #[test]
    fn test_error_code_with_details() {
        let err = TestAuthError.build_with_details("Token expired");
        assert_eq!(err.details.unwrap(), "Token expired");
    }
}
