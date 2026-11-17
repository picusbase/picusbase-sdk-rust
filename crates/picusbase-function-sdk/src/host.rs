//! What a function can ask of the platform: log lines, database queries and outbound HTTP.
//!
//! Each call is a one-shot JSON exchange with the compute mediator over TCP (write the request,
//! shut down the write side, read to EOF). The mediator works out which project the call belongs
//! to from the source address of the guest, so nothing in the payload names a tenant.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::TcpStream;

use crate::Error;

// Set once per request by the worker loop before it calls the handler. The loop handles one
// request at a time per process, so a thread-local is enough.
thread_local! {
    static MEDIATOR_ADDR: RefCell<String> = const { RefCell::new(String::new()) };
    static REQUEST_ID: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Called by the worker loop before it invokes the handler. Not part of the surface handlers use.
pub fn set_native_request_context(mediator_addr: &str, request_id: &str) {
    MEDIATOR_ADDR.with(|a| *a.borrow_mut() = mediator_addr.to_string());
    REQUEST_ID.with(|r| *r.borrow_mut() = request_id.to_string());
}

/// The context `set_native_request_context` set on this thread. A thread does not inherit it, so a
/// handler that spawns threads to call `query_read_only` or `http_request` in parallel reads it
/// here and calls `set_native_request_context` again inside each thread.
pub fn get_native_request_context() -> (String, String) {
    (MEDIATOR_ADDR.with(|a| a.borrow().clone()), REQUEST_ID.with(|r| r.borrow().clone()))
}

fn call_mediator(payload: serde_json::Value) -> Result<serde_json::Value, Error> {
    let addr = MEDIATOR_ADDR.with(|a| a.borrow().clone());
    if addr.is_empty() {
        return Err(Error::from("mediator address not set (set_native_request_context was not called)"));
    }
    let mut stream = TcpStream::connect(&addr).map_err(|e| Error::from(format!("mediator connect failed: {e}")))?;
    let body = serde_json::to_vec(&payload).map_err(|e| Error::from(format!("mediator payload encode failed: {e}")))?;
    stream.write_all(&body).map_err(|e| Error::from(format!("mediator write failed: {e}")))?;
    stream.shutdown(std::net::Shutdown::Write).map_err(|e| Error::from(format!("mediator shutdown(write) failed: {e}")))?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).map_err(|e| Error::from(format!("mediator read failed: {e}")))?;
    serde_json::from_slice(&buf).map_err(|e| Error::from(format!("bad mediator response: {e}")))
}

fn mediator_capability_call(capability: &str, extra: serde_json::Value) -> Result<serde_json::Value, Error> {
    let request_id = REQUEST_ID.with(|r| r.borrow().clone());
    let mut payload = serde_json::json!({
        "capability": capability,
        "request_id": request_id,
    });
    if let (Some(payload_obj), Some(extra_obj)) = (payload.as_object_mut(), extra.as_object()) {
        for (k, v) in extra_obj {
            payload_obj.insert(k.clone(), v.clone());
        }
    }
    let resp = call_mediator(payload)?;
    if resp.get("status").and_then(|v| v.as_str()) == Some("ok") {
        return Ok(resp);
    }
    let detail = resp
        .get("message")
        .and_then(|v| v.as_str())
        .or_else(|| resp.get("code").and_then(|v| v.as_str()))
        .map(str::to_string)
        .unwrap_or_else(|| resp.to_string());
    Err(Error::from(format!(
        "capability {capability} {}: {detail}",
        resp.get("status").and_then(|v| v.as_str()).unwrap_or("error")
    )))
}

pub fn log_debug(msg: &str) {
    eprintln!("[native-guest] DEBUG {msg}");
}

pub fn log_info(msg: &str) {
    eprintln!("[native-guest] INFO {msg}");
}

pub fn log_warn(msg: &str) {
    eprintln!("[native-guest] WARN {msg}");
}

pub fn log_error(msg: &str) {
    eprintln!("[native-guest] ERROR {msg}");
}

pub fn query_read_only(sql: &str, params: &[serde_json::Value]) -> Result<Vec<serde_json::Value>, Error> {
    let resp = mediator_capability_call("db.query.read", serde_json::json!({ "sql": sql, "params": params }))?;
    let payload = resp.get("payload").cloned().unwrap_or(serde_json::Value::Array(vec![]));
    serde_json::from_value(payload).map_err(|e| Error::from(format!("Failed to deserialize DB result: {e}")))
}

pub fn execute_write(sql: &str, params: &[serde_json::Value]) -> Result<Vec<serde_json::Value>, Error> {
    let resp = mediator_capability_call("db.query.write", serde_json::json!({ "sql": sql, "params": params }))?;
    let payload = resp.get("payload").cloned().unwrap_or(serde_json::Value::Array(vec![]));
    serde_json::from_value(payload).map_err(|e| Error::from(format!("Failed to deserialize DB result: {e}")))
}

pub fn http_request(
    method: &str,
    url: &str,
    headers: Option<&std::collections::HashMap<String, String>>,
    body: Option<&serde_json::Value>,
) -> Result<HttpResponse, Error> {
    let resp = mediator_capability_call(
        "http.fetch",
        serde_json::json!({
            "url": url,
            "method": method,
            "headers": headers.cloned().unwrap_or_default(),
            "body": body,
        }),
    )?;
    let payload = resp.get("payload").cloned().unwrap_or(serde_json::Value::Null);
    let status = payload.get("status").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
    let body = payload.get("body").cloned().unwrap_or(serde_json::Value::Null);
    Ok(HttpResponse { status, body })
}

/// Outbound HTTP response returned by `http_request`. Distinct from the function's own
/// `crate::HttpResponse`: it carries only the status and body, as the mediator returns no headers.
pub struct HttpResponse {
    pub status: u16,
    pub body: serde_json::Value,
}
