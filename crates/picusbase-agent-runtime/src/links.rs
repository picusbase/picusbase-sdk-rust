//! Sinh block `Link` điều hướng tới trang chi tiết của record — **do code quyết định, không phải
//! model**.
//!
//! Model chỉ cần nhắc tới bản ghi trong câu trả lời; đường link lấy từ khai báo tường minh
//! `x-picus-record-links` trong `openapi.yaml` của Function vừa gọi. Nhờ vậy model không thể tự
//! bịa `page_code` (nguồn ảo giác) và cũng không thể trỏ người dùng tới trang ngoài phạm vi đã
//! khai báo. Function không khai báo gì ⇒ không sinh link nào (fail-closed).

use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct LinkRef {
    pub entity_code: String,
    pub target_page_code: String,
    pub record_id: String,
    pub label: String,
}

impl LinkRef {
    pub fn to_block(&self) -> Value {
        json!({
            "id": format!("lnk_{}_{}", self.entity_code, self.record_id),
            "type": "Link",
            "props": {
                "label": self.label,
                "target_page_code": self.target_page_code,
                "record_id": self.record_id,
                // Bản ghi vừa tạo qua agent — mở tab mới để giữ khung chat đang xem, không điều
                // hướng rời khỏi hội thoại (bổ sung 2026-09-13, theo yêu cầu người dùng).
                "open_in_new_tab": true,
            }
        })
    }
}

/// Đối chiếu kết quả tool-call với khai báo `x-picus-record-links`.
///
/// Mỗi khai báo: `{ field, entity_code, target_page_code, label? }`. Tìm `field` ở cả 2 hình dạng
/// kết quả thường gặp: object phẳng (`{"invoice_id": "..."}`) và mảng bản ghi
/// (`{"projects": [{"project_id": "..."}]}`) — quét đệ quy nên không phụ thuộc tên khoá bao ngoài.
pub fn extract(openapi_yaml: &str, tool_result: &Value) -> Vec<LinkRef> {
    let Ok(doc) = serde_yaml::from_str::<Value>(openapi_yaml) else {
        return Vec::new();
    };
    let declarations = doc
        .get("paths")
        .and_then(Value::as_object)
        .and_then(|paths| {
            paths.values().find_map(|item| {
                item.get("post").and_then(|post| post.get("x-picus-record-links")).cloned()
            })
        })
        .unwrap_or(Value::Null);

    let mut links = Vec::new();
    for decl in declarations.as_array().into_iter().flatten() {
        let Some(field) = decl.get("field").and_then(Value::as_str) else { continue };
        let Some(entity_code) = decl.get("entity_code").and_then(Value::as_str) else { continue };
        let Some(page) = decl.get("target_page_code").and_then(Value::as_str) else { continue };
        let label_prefix = decl.get("label").and_then(Value::as_str).unwrap_or("Xem chi tiết");

        for id in collect_field_values(tool_result, field) {
            let link = LinkRef {
                entity_code: entity_code.to_string(),
                target_page_code: page.to_string(),
                record_id: id,
                label: label_prefix.to_string(),
            };
            if !links.contains(&link) {
                links.push(link);
            }
        }
    }
    links
}

/// Mọi giá trị chuỗi không rỗng của `field` ở bất kỳ độ sâu nào.
fn collect_field_values(value: &Value, field: &str) -> Vec<String> {
    let mut out = Vec::new();
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key == field {
                    if let Some(text) = child.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                        out.push(text.to_string());
                    }
                }
                out.extend(collect_field_values(child, field));
            }
        }
        Value::Array(items) => {
            for item in items {
                out.extend(collect_field_values(item, field));
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const WITH_LINKS: &str = r#"
paths:
  /functions/project_list_at_risk/execute:
    post:
      operationId: executeProjectListAtRisk
      description: Danh sách dự án trễ hạn.
      x-picus-record-links:
        - field: project_id
          entity_code: project
          target_page_code: project_detail_page
          label: "Mở dự án"
"#;

    const WITHOUT_LINKS: &str = r#"
paths:
  /functions/finance_reports/execute:
    post:
      operationId: executeFinanceReports
      description: Báo cáo.
"#;

    #[test]
    fn extracts_links_from_nested_array_result() {
        let result = serde_json::json!({
            "projects": [
                { "project_id": "uuid-1", "name": "A" },
                { "project_id": "uuid-2", "name": "B" }
            ]
        });
        let links = extract(WITH_LINKS, &result);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_page_code, "project_detail_page");
        assert_eq!(links[0].record_id, "uuid-1");
        assert_eq!(links[0].to_block()["props"]["label"], "Mở dự án");
    }

    #[test]
    fn no_declaration_means_no_links_even_if_ids_present() {
        // Đây là tính chất bảo mật: Function không khai báo thì không có link nào được sinh ra,
        // dù kết quả có đầy id.
        let result = serde_json::json!({ "project_id": "uuid-1", "invoice_id": "uuid-9" });
        assert!(extract(WITHOUT_LINKS, &result).is_empty());
    }
}
