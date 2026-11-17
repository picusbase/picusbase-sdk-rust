//! Gọi LLM qua capability `http.fetch`, che khác biệt định dạng giữa các nhà cung cấp.
//!
//! **Không xử lý xác thực**: mediator chèn header khoá theo host đích (xem
//! `compute-mediator::llm_auth_headers`). Crate này chỉ dựng body/đọc response.

use picusbase_function_sdk::{host, Error};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmProvider {
    Anthropic,
    OpenAi,
    Gemini,
    /// Model chạy tại chỗ qua Ollama — dùng API **native** `/api/chat`, KHÔNG dùng endpoint
    /// tương thích OpenAI của nó.
    ///
    /// Lý do (đo thật trên qwen3:8b, M1 Pro, cùng 1 prompt + 2 tool, 5 lần mỗi bên):
    ///   * `/v1/chat/completions`: tên tool luôn đúng nhưng **đối số sai 4/5 lần** — model nhét cả
    ///     cấu trúc schema vào giá trị (`{"report":{"type":"pnl"}}` thay vì `"pnl"`), 4–8s/lượt.
    ///   * `/api/chat` + `think:false`: **5/5 đối số đúng**, ~1s/lượt.
    /// Chênh lệch đủ lớn để chấp nhận viết thêm 1 lớp map thay vì tái dùng định dạng OpenAI.
    Local,
}

impl LlmProvider {
    /// Đọc từ `ai_agent.llm_provider`. Giá trị lạ ⇒ `None` để caller báo lỗi cấu hình rõ ràng,
    /// KHÔNG âm thầm rơi về 1 hãng mặc định (người dùng sẽ không hiểu vì sao trả lời khác hãng họ chọn).
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "anthropic" => Some(Self::Anthropic),
            "openai" => Some(Self::OpenAi),
            "gemini" | "google" => Some(Self::Gemini),
            "local" | "ollama" => Some(Self::Local),
            _ => None,
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::Anthropic => "claude-sonnet-5",
            Self::OpenAi => "gpt-5",
            Self::Gemini => "gemini-2.5-pro",
            // Qwen3 là dòng ổn định nhất về tool-calling trong nhóm chạy được tại chỗ (JSON đối số
            // hợp lệ gần như luôn đúng schema) — đúng thứ kiến trúc này phụ thuộc.
            Self::Local => "qwen3:8b",
        }
    }

    /// `base_url` chỉ dùng cho [`Self::Local`] — 3 nhà cung cấp đám mây có endpoint cố định, để
    /// chúng đọc từ tham số sẽ mở đường cho việc vô tình gửi khoá API tới host lạ.
    fn endpoint(self, model: &str, base_url: &str) -> String {
        match self {
            Self::Anthropic => "https://api.anthropic.com/v1/messages".to_string(),
            Self::OpenAi => "https://api.openai.com/v1/chat/completions".to_string(),
            // Gemini nhúng model vào URL thay vì body.
            Self::Gemini => format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
            ),
            Self::Local => format!("{}/api/chat", base_url.trim_end_matches('/')),
        }
    }
}

/// 1 lượt trong hội thoại. `Tool` = kết quả trả về cho 1 tool-call trước đó.
#[derive(Debug, Clone)]
pub enum LlmMessage {
    User(String),
    Assistant(String),
    Tool { tool_call_id: String, name: String, content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Default)]
pub struct TurnResult {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

/// Gọi 1 lượt LLM. `tools` là danh sách schema đã sinh từ `openapi.yaml` (xem [`crate::tools`]).
pub fn complete(
    provider: LlmProvider,
    model: &str,
    system_prompt: &str,
    messages: &[LlmMessage],
    tools: &[Value],
    local_base_url: &str,
) -> Result<TurnResult, Error> {
    let model = if model.trim().is_empty() { provider.default_model() } else { model.trim() };
    let body = match provider {
        LlmProvider::Anthropic => anthropic_body(model, system_prompt, messages, tools),
        LlmProvider::OpenAi => openai_body(model, system_prompt, messages, tools, false),
        LlmProvider::Gemini => gemini_body(system_prompt, messages, tools),
        LlmProvider::Local => ollama_body(model, system_prompt, messages, tools),
    };

    let mut headers = HashMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());

    let resp = host::http_request(
        "POST",
        &provider.endpoint(model, local_base_url),
        Some(&headers),
        Some(&body),
    )?;
    if !(200..300).contains(&resp.status) {
        // Giữ nguyên body lỗi của nhà cung cấp: thông điệp của họ (hết quota, model sai tên,
        // thiếu workspace…) là thứ duy nhất giúp chẩn đoán, đừng che bằng message chung chung.
        return Err(Error::from(format!(
            "LLM trả lỗi {} ({:?}): {}",
            resp.status, provider, resp.body
        )));
    }
    let payload = match resp.body {
        Value::String(raw) => serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null),
        other => other,
    };
    Ok(match provider {
        LlmProvider::Anthropic => parse_anthropic(&payload),
        LlmProvider::OpenAi => parse_openai(&payload),
        LlmProvider::Gemini => parse_gemini(&payload),
        LlmProvider::Local => parse_ollama(&payload),
    })
}

const MAX_TOKENS: u32 = 4096;

fn anthropic_body(
    model: &str,
    system_prompt: &str,
    messages: &[LlmMessage],
    tools: &[Value],
) -> Value {
    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| match m {
            LlmMessage::User(text) => json!({ "role": "user", "content": text }),
            LlmMessage::Assistant(text) => json!({ "role": "assistant", "content": text }),
            // Anthropic nhận tool-result như 1 message của `user` với block `tool_result`.
            LlmMessage::Tool { tool_call_id, content, .. } => json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": content,
                }]
            }),
        })
        .collect();
    let mut body = json!({ "model": model, "max_tokens": MAX_TOKENS, "messages": msgs });
    if !system_prompt.trim().is_empty() {
        body["system"] = json!(system_prompt);
    }
    if !tools.is_empty() {
        body["tools"] = json!(tools
            .iter()
            .map(|t| json!({
                "name": t.get("name").cloned().unwrap_or(Value::Null),
                "description": t.get("description").cloned().unwrap_or(Value::Null),
                "input_schema": t.get("parameters").cloned().unwrap_or(json!({"type": "object"})),
            }))
            .collect::<Vec<_>>());
    }
    body
}

/// `legacy_max_tokens`: Ollama (và các server tương thích OpenAI khác) chỉ hiểu `max_tokens`;
/// `max_completion_tokens` là tham số mới chỉ OpenAI thật hỗ trợ. Gửi sai tên thì server hoặc
/// báo lỗi hoặc âm thầm bỏ giới hạn — cả 2 đều tệ.
fn openai_body(
    model: &str,
    system_prompt: &str,
    messages: &[LlmMessage],
    tools: &[Value],
    legacy_max_tokens: bool,
) -> Value {
    let mut msgs: Vec<Value> = Vec::with_capacity(messages.len() + 1);
    if !system_prompt.trim().is_empty() {
        msgs.push(json!({ "role": "system", "content": system_prompt }));
    }
    for m in messages {
        msgs.push(match m {
            LlmMessage::User(text) => json!({ "role": "user", "content": text }),
            LlmMessage::Assistant(text) => json!({ "role": "assistant", "content": text }),
            LlmMessage::Tool { tool_call_id, content, .. } => {
                json!({ "role": "tool", "tool_call_id": tool_call_id, "content": content })
            }
        });
    }
    let token_key = if legacy_max_tokens { "max_tokens" } else { "max_completion_tokens" };
    let mut body = json!({ "model": model, "messages": msgs });
    body[token_key] = json!(MAX_TOKENS);
    if !tools.is_empty() {
        body["tools"] = json!(tools
            .iter()
            .map(|t| json!({ "type": "function", "function": t }))
            .collect::<Vec<_>>());
    }
    body
}

/// Body cho Ollama `/api/chat`.
///
/// `think: false` là bắt buộc: bật thinking (mặc định của Qwen3) làm model tiêu hết token vào phần
/// suy luận rồi **dừng mà không phát tool-call** — đo thật: cùng prompt, bật thinking ra 0 tool-call
/// dù phần suy luận nói rõ "I'll call executeFinanceReports".
fn ollama_body(model: &str, system_prompt: &str, messages: &[LlmMessage], tools: &[Value]) -> Value {
    let mut msgs: Vec<Value> = Vec::with_capacity(messages.len() + 1);
    if !system_prompt.trim().is_empty() {
        msgs.push(json!({ "role": "system", "content": system_prompt }));
    }
    for m in messages {
        msgs.push(match m {
            LlmMessage::User(text) => json!({ "role": "user", "content": text }),
            LlmMessage::Assistant(text) => json!({ "role": "assistant", "content": text }),
            // Ollama nhận tool-result bằng `role: tool` + `content` thuần (không cần tool_call_id).
            LlmMessage::Tool { content, .. } => json!({ "role": "tool", "content": content }),
        });
    }
    let mut body = json!({
        "model": model,
        "stream": false,
        "think": false,
        "messages": msgs,
        // `temperature: 0` giảm (KHÔNG loại bỏ) việc model tự bịa số liệu khi tổng hợp câu
        // trả lời — đo thật: cùng ngữ cảnh có tool-result đúng, model 8B mặc định vẫn thỉnh
        // thoảng (~1/12 lần trong thử nghiệm) viết số khác hẳn số thật dù số thật nằm ngay
        // trong context. Không có cách nào loại bỏ hoàn toàn rủi ro này ở model 8B — đây là lý
        // do Mục 3 của PLAN yêu cầu số liệu hiển thị qua block (Chart/EntityAggregate, dựng từ
        // dữ liệu thật) chứ không tin vào lời văn của model cho số chính xác.
        "options": { "num_predict": MAX_TOKENS, "temperature": 0.0 },
    });
    if !tools.is_empty() {
        body["tools"] = json!(tools
            .iter()
            .map(|t| json!({ "type": "function", "function": t }))
            .collect::<Vec<_>>());
    }
    body
}

fn gemini_body(system_prompt: &str, messages: &[LlmMessage], tools: &[Value]) -> Value {
    let contents: Vec<Value> = messages
        .iter()
        .map(|m| match m {
            LlmMessage::User(text) => json!({ "role": "user", "parts": [{ "text": text }] }),
            LlmMessage::Assistant(text) => json!({ "role": "model", "parts": [{ "text": text }] }),
            LlmMessage::Tool { name, content, .. } => json!({
                "role": "user",
                "parts": [{ "functionResponse": { "name": name, "response": { "result": content } } }]
            }),
        })
        .collect();
    let mut body = json!({ "contents": contents });
    if !system_prompt.trim().is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": system_prompt }] });
    }
    if !tools.is_empty() {
        body["tools"] = json!([{ "functionDeclarations": tools }]);
    }
    body
}

fn parse_anthropic(payload: &Value) -> TurnResult {
    let mut out = TurnResult::default();
    for block in payload.get("content").and_then(Value::as_array).into_iter().flatten() {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    out.content.push_str(text);
                }
            }
            Some("tool_use") => out.tool_calls.push(ToolCall {
                id: block.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
                name: block.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                arguments: block.get("input").cloned().unwrap_or(json!({})),
            }),
            _ => {}
        }
    }
    out
}

fn parse_openai(payload: &Value) -> TurnResult {
    let mut out = TurnResult::default();
    let Some(message) = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("message"))
    else {
        return out;
    };
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        out.content.push_str(text);
    }
    for call in message.get("tool_calls").and_then(Value::as_array).into_iter().flatten() {
        let function = call.get("function");
        // OpenAI trả `arguments` là CHUỖI JSON, không phải object — parse lại, lỗi thì để object
        // rỗng (caller sẽ báo thiếu tham số, tốt hơn là bỏ luôn tool-call làm agent treo im lặng).
        let arguments = function
            .and_then(|f| f.get("arguments"))
            .and_then(Value::as_str)
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or(json!({}));
        out.tool_calls.push(ToolCall {
            id: call.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
            name: function
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            arguments,
        });
    }
    out
}

fn parse_ollama(payload: &Value) -> TurnResult {
    let mut out = TurnResult::default();
    let Some(message) = payload.get("message") else {
        return out;
    };
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        out.content.push_str(strip_think_blocks(text).trim());
    }
    for (index, call) in
        message.get("tool_calls").and_then(Value::as_array).into_iter().flatten().enumerate()
    {
        let function = call.get("function");
        let name = function
            .and_then(|f| f.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        out.tool_calls.push(ToolCall {
            // Ollama không phải lúc nào cũng phát `id` — tự sinh để khớp tool-result về sau.
            id: call
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("ollama-{index}-{name}")),
            name,
            // KHÁC OpenAI: `arguments` ở đây đã là object, không phải chuỗi JSON.
            arguments: function.and_then(|f| f.get("arguments")).cloned().unwrap_or(json!({})),
        });
    }
    out
}

/// Bỏ phần suy luận `<think>…</think>` lẫn trong nội dung.
///
/// Dù đã gửi `think: false`, Qwen3 vẫn có lúc tự sinh khối `<think>` và Ollama trả thẳng vào
/// `content` (gặp thật: câu trả lời lưu xuống DB dính cả đoạn suy luận + thẻ `</think>` lạc). Suy
/// luận thường chứa **số liệu model tự bịa** nên để lọt ra người dùng là nguy hiểm, không chỉ xấu.
/// Xử lý cả ca thiếu thẻ mở (chỉ còn `</think>` — đúng ca đã gặp).
fn strip_think_blocks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        match rest.find("<think>") {
            Some(start) => {
                out.push_str(&rest[..start]);
                match rest[start..].find("</think>") {
                    Some(end) => rest = &rest[start + end + "</think>".len()..],
                    // Thẻ mở không đóng ⇒ bỏ toàn bộ phần còn lại (là suy luận dở dang).
                    None => return out,
                }
            }
            None => {
                // Không có thẻ mở: nếu vẫn còn thẻ đóng lạc thì mọi thứ trước nó là suy luận.
                match rest.rfind("</think>") {
                    Some(end) => out.push_str(&rest[end + "</think>".len()..]),
                    None => out.push_str(rest),
                }
                return out;
            }
        }
    }
}

fn parse_gemini(payload: &Value) -> TurnResult {
    let mut out = TurnResult::default();
    let parts = payload
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(Value::as_array);
    for (index, part) in parts.into_iter().flatten().enumerate() {
        if let Some(text) = part.get("text").and_then(Value::as_str) {
            out.content.push_str(text);
        }
        if let Some(call) = part.get("functionCall") {
            let name = call.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            out.tool_calls.push(ToolCall {
                // Gemini không phát id cho function call — tự sinh để khớp tool-result về sau.
                id: format!("gemini-{index}-{name}"),
                name,
                arguments: call.get("args").cloned().unwrap_or(json!({})),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_anthropic_text_and_tool_use() {
        let payload = json!({
            "content": [
                { "type": "text", "text": "Đang kiểm tra" },
                { "type": "tool_use", "id": "tu_1", "name": "finance_reports", "input": { "kind": "pnl" } }
            ]
        });
        let out = parse_anthropic(&payload);
        assert_eq!(out.content, "Đang kiểm tra");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "finance_reports");
        assert_eq!(out.tool_calls[0].arguments["kind"], "pnl");
    }

    #[test]
    fn parses_openai_stringified_arguments() {
        let payload = json!({
            "choices": [{ "message": {
                "content": null,
                "tool_calls": [{ "id": "call_1", "function": { "name": "f", "arguments": "{\"a\":1}" } }]
            }}]
        });
        let out = parse_openai(&payload);
        assert_eq!(out.tool_calls[0].arguments["a"], 1);
    }

    #[test]
    fn parses_ollama_object_arguments() {
        // Ollama trả `arguments` dạng OBJECT (khác OpenAI trả chuỗi JSON) — parse sai chỗ này thì
        // mọi tool-call của model local đều mất tham số.
        let payload = json!({ "message": {
            "content": "",
            "tool_calls": [{ "function": { "name": "executeFinanceReports", "arguments": { "report": "pnl" } } }]
        }});
        let out = parse_ollama(&payload);
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].arguments["report"], "pnl");
        assert!(!out.tool_calls[0].id.is_empty(), "phải tự sinh id khi Ollama không trả");
    }

    #[test]
    fn strips_leaked_thinking_from_content() {
        // Ca đã gặp thật: chỉ còn thẻ đóng, phần trước nó là suy luận chứa SỐ LIỆU BỊA.
        let leaked = "1,2 tỷ, đạt 95% kế hoạch.\n</think>\nRất tiếc, tôi cần thêm thông tin.";
        assert_eq!(strip_think_blocks(leaked).trim(), "Rất tiếc, tôi cần thêm thông tin.");
        // Khối đầy đủ.
        assert_eq!(strip_think_blocks("<think>suy luận</think>Đáp án").trim(), "Đáp án");
        // Thẻ mở không đóng ⇒ bỏ hết phần sau.
        assert_eq!(strip_think_blocks("Đáp án<think>dở dang").trim(), "Đáp án");
        // Văn bản thường giữ nguyên.
        assert_eq!(strip_think_blocks("Bình thường").trim(), "Bình thường");
    }

    #[test]
    fn ollama_body_disables_thinking() {
        // Bật thinking ⇒ model dừng mà không phát tool-call (đo thật) — đây là bảo vệ hồi quy.
        let body = ollama_body("qwen3:8b", "sys", &[LlmMessage::User("hi".into())], &[]);
        assert_eq!(body["think"], false);
        assert_eq!(body["stream"], false);
        assert_eq!(body["options"]["num_predict"], MAX_TOKENS);
    }

    #[test]
    fn local_uses_legacy_max_tokens_key() {
        // Ollama chỉ hiểu `max_tokens`; gửi `max_completion_tokens` sẽ bị bỏ qua/báo lỗi.
        let legacy = openai_body("m", "sys", &[LlmMessage::User("hi".into())], &[], true);
        assert!(legacy.get("max_tokens").is_some());
        assert!(legacy.get("max_completion_tokens").is_none());
        let cloud = openai_body("gpt-5", "sys", &[LlmMessage::User("hi".into())], &[], false);
        assert!(cloud.get("max_completion_tokens").is_some());
        assert!(cloud.get("max_tokens").is_none());
    }

    #[test]
    fn unknown_provider_is_rejected_not_defaulted() {
        assert_eq!(LlmProvider::parse("anthropic"), Some(LlmProvider::Anthropic));
        assert_eq!(LlmProvider::parse("khong-ton-tai"), None);
        assert_eq!(LlmProvider::parse("local"), Some(LlmProvider::Local));
        assert_eq!(LlmProvider::parse("ollama"), Some(LlmProvider::Local));
    }

    #[test]
    fn only_local_provider_honours_base_url() {
        // Bảo vệ: nhà cung cấp đám mây KHÔNG được đọc base_url, tránh gửi khoá API tới host lạ.
        let evil = "http://evil.example.com";
        assert!(LlmProvider::Anthropic.endpoint("m", evil).starts_with("https://api.anthropic.com"));
        assert!(LlmProvider::OpenAi.endpoint("m", evil).starts_with("https://api.openai.com"));
        assert!(LlmProvider::Gemini.endpoint("m", evil).starts_with("https://generativelanguage."));
        assert_eq!(
            LlmProvider::Local.endpoint("m", "http://host.docker.internal:11434/"),
            "http://host.docker.internal:11434/api/chat"
        );
    }
}
