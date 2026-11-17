//! Sinh block `ChatConfirm` (nút "Xác nhận"/"Huỷ") khi 1 tool trả `awaiting_confirmation: true` —
//! CÙNG triết lý `links.rs`: CODE quyết định hiển thị nút gì, model không tự vẽ UI xác nhận.
//!
//! Lý do tồn tại (2026-09-13, đã kiểm chứng thật trên `qwen3:8b`): dựa vào việc model tự gõ lại
//! đúng nội dung xem trước rồi CHỜ người dùng gõ "có"/"không" là không đủ tin cậy — model có thể tự
//! bịa "đã lưu thành công" mà không hề gọi lại tool để xác nhận. Bấm nút thay vì gõ chữ vừa dứt
//! khoát hơn với người dùng, vừa loại bỏ khâu model phải tự nhớ gọi lại đúng tool.
use serde_json::{json, Value};

/// Function phải khai `x-picus-confirm-action: true` trên operation mới được sinh nút — fail-closed
/// như `x-picus-record-links`: 1 field tên trùng ngẫu nhiên trong response của function khác không
/// tự nhiên kích hoạt UI xác nhận.
pub fn extract(openapi_yaml: &str, function_code: &str, tool_result: &Value) -> Option<Value> {
    if !tool_result.get("awaiting_confirmation").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    let doc: Value = serde_yaml::from_str(openapi_yaml).ok()?;
    let declared = doc
        .get("paths")?
        .as_object()?
        .values()
        .find_map(|item| item.get("post")?.get("x-picus-confirm-action")?.as_bool())
        .unwrap_or(false);
    if !declared {
        return None;
    }
    let summary = tool_result.get("summary").and_then(Value::as_str).unwrap_or_default().to_string();
    Some(json!({
        "id": format!("confirm_{function_code}"),
        "type": "ChatConfirm",
        "props": { "function_code": function_code, "summary": summary },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENAPI: &str = r#"
openapi: "3.0.3"
paths:
  /functions/x/execute:
    post:
      x-picus-confirm-action: true
      operationId: executeX
"#;

    #[test]
    fn generates_block_when_awaiting_confirmation_and_declared() {
        let result = json!({ "ok": true, "awaiting_confirmation": true, "summary": "Sẽ tạo X" });
        let block = extract(OPENAPI, "inventory_create_warehouse", &result).expect("phải sinh block");
        assert_eq!(block["type"], "ChatConfirm");
        assert_eq!(block["props"]["summary"], "Sẽ tạo X");
    }

    #[test]
    fn no_block_without_awaiting_confirmation() {
        let result = json!({ "ok": true, "id": "abc" });
        assert!(extract(OPENAPI, "x", &result).is_none());
    }

    #[test]
    fn no_block_without_declaration() {
        let no_decl = r#"
openapi: "3.0.3"
paths:
  /functions/x/execute:
    post:
      operationId: executeX
"#;
        let result = json!({ "ok": true, "awaiting_confirmation": true, "summary": "Sẽ tạo X" });
        assert!(extract(no_decl, "x", &result).is_none());
    }
}
