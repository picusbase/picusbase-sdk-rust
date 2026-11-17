//! The entrypoint shared by every native-v1 function, so each function's
//! `src/bin/native_worker.rs` is one line instead of a copy of this loop.
//!
//! Protocol: newline-delimited JSON on stdin and stdout, one request in, one response out, the
//! same envelope the JavaScript and Python guest shims use. The platform's native shim pipes the
//! dispatcher's request bytes through without translating them.

use crate::{host, ExecutionContext, HttpHandler, HttpRequest};
use serde_json::Value;
use std::io::{self, BufRead, Write};

/// The platform injects `PICUSBASE_<name>` (it was `PICUS_<name>` before 2026-11-16); the old name is read when the new
/// one is unset, so a build keeps working on a cluster that has not picked up the rename. Reading only the old name
/// leaves the mediator address empty and fails every database or HTTP call with "mediator address not set".
fn env_var(new_name: &str, old_name: &str) -> Option<String> {
    std::env::var(new_name).ok().or_else(|| std::env::var(old_name).ok())
}

pub fn run_native_worker<H: HttpHandler>() {
    let mediator_addr = env_var("PICUSBASE_MEDIATOR_TCP_ADDR", "PICUS_MEDIATOR_TCP_ADDR").unwrap_or_default();
    let project_ref = env_var("PICUSBASE_PROJECT_REF", "PICUS_PROJECT_REF").unwrap_or_else(|| "unknown".to_string());
    let function_id = env_var("PICUSBASE_FUNCTION_ID", "PICUS_FUNCTION_ID").unwrap_or_else(|| "unknown".to_string());

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            Ok(_) => continue,
            Err(_) => break,
        };

        let raw: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let _ = writeln!(out, "{}", serde_json::json!({"status": "error", "error": format!("bad request json: {e}")}));
                let _ = out.flush();
                continue;
            }
        };

        let request_id = raw.get("request_id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        host::set_native_request_context(&mediator_addr, &request_id);

        let req = HttpRequest {
            method: raw.get("method").and_then(|v| v.as_str()).unwrap_or("GET").to_string(),
            path: raw.get("path").and_then(|v| v.as_str()).unwrap_or("/").to_string(),
            path_params: raw.get("path_params").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default(),
            query: raw.get("query").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default(),
            headers: raw.get("headers").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default(),
            body: raw.get("body").cloned(),
        };
        let ctx = ExecutionContext {
            execution_id: request_id.clone(),
            function_key: function_id.clone(),
            project_ref: project_ref.clone(),
            user_id: None,
            user_role: None,
        };

        let response = match H::handle(req, ctx) {
            Ok(response) => serde_json::json!({
                "request_id": request_id,
                "status": "ok",
                "result": {
                    "status": response.status,
                    "headers": response.headers,
                    "body": response.body,
                },
            }),
            Err(e) => serde_json::json!({
                "request_id": request_id,
                "status": "error",
                "error": e.message,
            }),
        };

        let _ = writeln!(out, "{}", response);
        let _ = out.flush();
    }
}
