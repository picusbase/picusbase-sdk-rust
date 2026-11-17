//! Lọc `blocks` do model sinh ra trước khi lưu/hiển thị.
//!
//! Fail-closed: chỉ giữ block có `type` nằm trong whitelist. Một model bị dẫn dụ (prompt
//! injection qua dữ liệu nghiệp vụ) không được phép vẽ block ngoài danh sách cho phép.

use serde_json::Value;

/// Whitelist mặc định cho khung chat — nhỏ hơn tập block dùng cho trang builder.
pub const CHAT_BLOCK_WHITELIST: &[&str] =
    &["Text", "Badge", "ProgressBar", "EntityAggregate", "Chart", "Link"];

/// Trả về danh sách block hợp lệ. Block sai kiểu/thiếu `type`/ngoài whitelist bị **bỏ**, không chỉ
/// cảnh báo. `None`/không phải mảng ⇒ danh sách rỗng.
pub fn validate(blocks_json: &Value, allowed_types: &[&str]) -> Vec<Value> {
    blocks_json
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("type")
                        .and_then(Value::as_str)
                        .map(|t| allowed_types.contains(&t))
                        .unwrap_or(false)
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn drops_blocks_outside_whitelist() {
        let blocks = json!([
            { "type": "Text", "props": { "text": "ok" } },
            { "type": "EntityForm", "props": { "entity_code": "customer" } },
            { "props": { "text": "thieu type" } },
            { "type": "Chart", "props": { "chart_type": "bar" } }
        ]);
        let kept = validate(&blocks, CHAT_BLOCK_WHITELIST);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0]["type"], "Text");
        assert_eq!(kept[1]["type"], "Chart");
    }

    #[test]
    fn non_array_yields_empty() {
        assert!(validate(&json!({"type": "Text"}), CHAT_BLOCK_WHITELIST).is_empty());
        assert!(validate(&Value::Null, CHAT_BLOCK_WHITELIST).is_empty());
    }
}
