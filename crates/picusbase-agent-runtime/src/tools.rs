//! Sinh schema tool-call cho LLM từ chính `openapi.yaml` của mỗi Picus Function.
//!
//! Không phát minh định dạng mô tả tool mới: mọi Function đã có `openapi.yaml` (dùng cho
//! route-binding), nên đó là nguồn duy nhất. Hệ quả: chất lượng chọn tool của agent phụ thuộc
//! trực tiếp vào chất lượng `description`/`requestBody` trong file đó — Function muốn cho agent
//! gọi thì phải viết mô tả nghiệp vụ đầy đủ, crate này KHÔNG tự đoán hộ.

use serde_json::{json, Map, Value};

/// Tên tool đặc biệt để giao việc cho trợ lý cấp dưới — không phải 1 Function ERP.
pub const DELEGATE_TOOL_NAME: &str = "delegate_to_subordinate";

#[derive(Debug, Clone)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    /// Đường gọi thật (path trong `openapi.yaml`) — Function dùng để biết gọi route nào.
    pub path: String,
}

impl ToolSchema {
    /// Dạng JSON chung mà [`crate::llm`] map tiếp sang từng nhà cung cấp.
    pub fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "parameters": self.parameters,
        })
    }
}

/// Đọc `openapi.yaml` của 1 Function, sinh [`ToolSchema`].
///
/// Lấy operation POST đầu tiên tìm thấy (mọi Function Picus đều expose đúng 1 route
/// `POST /functions/<id>/execute`). `description` ưu tiên `description` của operation, thiếu thì
/// dùng `summary`, thiếu cả hai ⇒ `Err`: một tool không có mô tả thì model không có cơ sở nào để
/// biết khi nào nên gọi — thà báo lỗi cấu hình còn hơn để agent đoán bừa.
pub fn from_openapi(openapi_yaml: &str) -> Result<ToolSchema, String> {
    let doc: Value = serde_yaml::from_str(openapi_yaml)
        .map_err(|e| format!("openapi.yaml không parse được: {e}"))?;

    let paths = doc.get("paths").and_then(Value::as_object).ok_or("openapi.yaml thiếu `paths`")?;
    let (path, item) = paths
        .iter()
        .find(|(_, item)| item.get("post").is_some())
        .ok_or("openapi.yaml không có operation POST nào")?;
    let post = item.get("post").expect("đã lọc bằng is_some ở trên");

    let name = post
        .get("operationId")
        .and_then(Value::as_str)
        .map(str::to_string)
        // `operationId` là cách đặt tên tool ổn định nhất; thiếu thì suy từ path
        // `/functions/<id>/execute`.
        .or_else(|| {
            path.trim_matches('/').split('/').nth(1).map(str::to_string)
        })
        .ok_or("không xác định được tên tool (thiếu operationId)")?;

    let description = post
        .get("description")
        .and_then(Value::as_str)
        .or_else(|| post.get("summary").and_then(Value::as_str))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            format!("operation `{name}` thiếu cả `description` và `summary` — không thể làm tool")
        })?;

    Ok(ToolSchema {
        name,
        description,
        parameters: request_body_schema(post),
        path: path.to_string(),
    })
}

/// Lấy JSON Schema của request body. Không có `requestBody` ⇒ object rỗng (tool không tham số).
fn request_body_schema(post: &Value) -> Value {
    let schema = post
        .get("requestBody")
        .and_then(|rb| rb.get("content"))
        .and_then(|c| c.get("application/json"))
        .and_then(|j| j.get("schema"))
        .cloned();
    match schema {
        Some(mut schema) => {
            // Một số provider từ chối schema thiếu `type` — chuẩn hoá về object.
            if schema.get("type").is_none() {
                if let Some(map) = schema.as_object_mut() {
                    map.insert("type".to_string(), json!("object"));
                }
            }
            schema
        }
        None => json!({ "type": "object", "properties": {} }),
    }
}

/// Schema cố định cho tool giao việc xuống cấp dưới.
///
/// `target_agent` nhận TÊN trợ lý cấp dưới (không phải UUID): model không nhìn thấy id nội bộ, và
/// bắt nó nhắc đúng tên giúp Function tra lại an toàn trong tập cấp dưới hợp lệ.
pub fn delegate_schema(subordinate_names: &[String]) -> Value {
    let mut properties = Map::new();
    properties.insert(
        "target_agent".to_string(),
        json!({
            "type": "string",
            "description": "Tên trợ lý cấp dưới sẽ nhận việc.",
            "enum": subordinate_names,
        }),
    );
    properties.insert(
        "task_description".to_string(),
        json!({
            "type": "string",
            "description": "Mô tả rõ việc cần cấp dưới làm, đủ ngữ cảnh để họ tự làm không cần hỏi lại.",
        }),
    );
    json!({
        "name": DELEGATE_TOOL_NAME,
        "description": "Giao 1 việc cho trợ lý cấp dưới khi việc đó nằm ngoài phạm vi/quyền của mình. \
                        Cấp dưới làm XONG NGAY trong lượt này và kết quả trả lại cho bạn để tổng hợp \
                        — không phải chờ lượt sau.",
        "parameters": {
            "type": "object",
            "properties": properties,
            "required": ["target_agent", "task_description"],
        },
    })
}

pub const FORM_TOOL_NAME: &str = "request_entity_form";

/// Schema cố định cho tool "hiện biểu mẫu nhập liệu trong chat".
///
/// Có tool này vì hỏi từng trường bằng lời rất tệ trên thực tế: model tự nghĩ ra danh sách trường
/// nên hay THIẾU trường bắt buộc hoặc BỊA trường không tồn tại, và người dùng phải gõ lại mọi thứ
/// dưới dạng văn xuôi để model đoán tiếp. Biểu mẫu lấy đúng trường + ràng buộc bắt buộc từ định
/// nghĩa entity (nguồn duy nhất là YAML đã deploy), người dùng điền rồi bấm lưu — ghi bằng QUYỀN
/// CỦA CHÍNH NGƯỜI DÙNG, không phải quyền trợ lý.
pub fn form_schema(entity_codes: &[String]) -> Value {
    let mut properties = Map::new();
    properties.insert(
        "entity".to_string(),
        json!({
            "type": "string",
            "description": "Loại dữ liệu cần tạo bản ghi mới.",
            "enum": entity_codes,
        }),
    );
    properties.insert(
        "record".to_string(),
        json!({
            "type": "string",
            "description": "Chỉ điền khi SỬA một bản ghi đã có: TÊN hoặc MÃ của bản ghi đó, chép đúng \
                            như người dùng gọi (vd \"Nguyễn Văn Tiến 2\", \"ti333000\"). Hệ thống tự \
                            tra ra bản ghi, bạn KHÔNG cần biết id. Bỏ trống = tạo bản ghi MỚI.",
        }),
    );
    properties.insert(
        "note".to_string(),
        json!({
            "type": "string",
            "description": "Một câu ngắn nói cho người dùng biết cần điền gì (tuỳ chọn).",
        }),
    );
    json!({
        "name": FORM_TOOL_NAME,
        "description": "Hiện BIỂU MẪU nhập liệu ngay trong khung chat để người dùng tự điền rồi bấm lưu. \
                        DÙNG KHI người dùng muốn THÊM MỚI một bản ghi, hoặc muốn SỬA một bản ghi đã có \
                        (khi sửa thì truyền `record` = tên/mã bản ghi, hệ thống tự tìm). \
                        Biểu mẫu tự hiện đúng mọi trường và tự bắt buộc các trường bắt buộc, nên TUYỆT ĐỐI \
                        KHÔNG tự liệt kê các trường bằng lời và không hỏi từng trường một — chỉ gọi tool này \
                        rồi nói 1 câu mời người dùng điền.",
        "parameters": {
            "type": "object",
            "properties": properties,
            "required": ["entity"],
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RICH: &str = r#"
openapi: "3.0.3"
info: { title: finance_reports, version: "1.0.0" }
paths:
  /functions/finance_reports/execute:
    post:
      summary: Báo cáo tài chính
      description: Dùng khi cần số liệu tài chính tổng hợp của kỳ.
      operationId: executeFinanceReports
      requestBody:
        content:
          application/json:
            schema:
              properties:
                report: { type: string, enum: [pnl, balance_sheet] }
              required: [report]
"#;

    const NO_DESC: &str = r#"
openapi: "3.0.3"
paths:
  /functions/x/execute:
    post:
      operationId: executeX
"#;

    #[test]
    fn derives_tool_from_enriched_openapi() {
        let tool = from_openapi(RICH).expect("phải parse được");
        assert_eq!(tool.name, "executeFinanceReports");
        assert!(tool.description.contains("số liệu tài chính"));
        assert_eq!(tool.parameters["type"], "object");
        assert_eq!(tool.parameters["properties"]["report"]["type"], "string");
        assert_eq!(tool.path, "/functions/finance_reports/execute");
    }

    #[test]
    fn rejects_operation_without_any_description() {
        // Thà lỗi cấu hình rõ ràng hơn là đưa cho model 1 tool không mô tả.
        let err = from_openapi(NO_DESC).expect_err("phải lỗi");
        assert!(err.contains("description"), "{err}");
    }

    #[test]
    fn delegate_schema_limits_target_to_known_subordinates() {
        let schema = delegate_schema(&["Trợ lý Phân tích Dự án".to_string()]);
        assert_eq!(schema["parameters"]["properties"]["target_agent"]["enum"][0], "Trợ lý Phân tích Dự án");
    }
}
