//! `picusbase-function-sdk` — the Rust side of a native-v1 PicusBase Function.
//!
//! A function implements [`HttpHandler`] and its `src/bin/native_worker.rs` is
//! `fn main() { picusbase_function_sdk::native::run_native_worker::<MyHandler>() }`.
//! Inside a handler, [`host`] reaches the platform (database, outbound HTTP) through the
//! compute mediator.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod host;
pub mod native;

/// The JSON value type used across the request and response envelopes.
pub use serde_json::Value as JsonValue;

/// HTTP request envelope passed from the platform to the function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub path_params: HashMap<String, String>,
    #[serde(default)]
    pub query: HashMap<String, String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub body: Option<JsonValue>,
}

/// Execution metadata injected by the platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub execution_id: String,
    pub function_key: String,
    pub project_ref: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub user_role: Option<String>,
}

/// HTTP response envelope returned by the function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub body: JsonValue,
}

/// Error type for execution.
#[derive(Debug)]
pub struct Error {
    pub message: String,
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self { message: message.to_string() }
    }
}

/// The logic of an HTTP-triggered function.
pub trait HttpHandler {
    fn handle(req: HttpRequest, ctx: ExecutionContext) -> Result<HttpResponse, Error>;
}
