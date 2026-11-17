//! Vòng lặp 1 "lượt suy nghĩ" của trợ lý: gọi LLM → chạy tool → gọi lại LLM → … → câu trả lời.
//!
//! Crate không tự gọi Function và không tự tạo job: caller truyền 2 closure để giữ mọi hiểu biết
//! về schema/hạ tầng của app ở phía Function (xem ghi chú ranh giới ở [`crate`]).

use crate::llm::{self, LlmMessage, LlmProvider, TurnResult};
use crate::{blocks, confirm, links, tools};
use picusbase_function_sdk::Error;
use serde_json::{json, Value};

/// Trần số vòng LLM↔tool trong 1 lượt. Mỗi vòng tốn 1 `http.fetch` cho LLM + 1 cho mỗi tool-call,
/// trong khi `CAPABILITY_CALL_BUDGET` mặc định là 16 — để 4 nhằm còn chỗ cho các lệnh đọc/ghi DB
/// của chính Function. Hết vòng mà model vẫn gọi tool ⇒ trả phần đã có kèm ghi chú, KHÔNG lặp vô hạn.
pub const MAX_TOOL_ROUNDS: usize = 4;

pub struct TurnInput<'a> {
    pub provider: LlmProvider,
    pub model: &'a str,
    pub system_prompt: &'a str,
    /// Lịch sử hội thoại (cũ → mới), đã do Function đọc từ bảng của app.
    pub messages: Vec<LlmMessage>,
    /// Tool từ Function ERP: `(schema, openapi_yaml)` — `openapi_yaml` giữ lại để `links::extract`
    /// đọc khai báo `x-picus-record-links` sau khi tool chạy.
    pub tools: Vec<(tools::ToolSchema, String)>,
    /// Tên các trợ lý cấp dưới; rỗng ⇒ không cấp tool giao việc.
    pub subordinates: Vec<String>,
    /// `entity_code` mà trợ lý này được phép mở biểu mẫu nhập liệu trong chat; rỗng ⇒ không cấp
    /// tool [`tools::FORM_TOOL_NAME`].
    pub form_entities: Vec<String>,
    pub allowed_block_types: &'a [&'a str],
    /// Base URL cho model chạy tại chỗ — bỏ qua với nhà cung cấp đám mây.
    pub local_base_url: &'a str,
}

#[derive(Debug, Default)]
pub struct TurnOutcome {
    pub content: String,
    /// Block đã qua whitelist + block `Link` do code sinh.
    pub blocks: Vec<Value>,
    /// Việc đã giao cho cấp dưới trong lượt này: `(tên cấp dưới, mô tả việc)`.
    pub delegations: Vec<(String, String)>,
    pub tool_calls_made: Vec<String>,
    /// `true` nếu dừng vì hết `MAX_TOOL_ROUNDS` — caller nên cho chạy thêm 1 lượt nữa.
    pub hit_round_limit: bool,
}

/// `call_tool(tool_name, arguments) -> kết quả JSON`; `enqueue_delegation(tên cấp dưới, mô tả việc)
/// -> nội dung câu trả lời của cấp dưới` — ĐỒNG BỘ (2026-09-13, đổi từ bất đồng bộ "chờ lượt sau"):
/// caller (`ai_agent_turn`) tự chạy NGAY lượt của cấp dưới trong lúc xử lý tool-call này rồi trả
/// lại nội dung, để cấp trên tổng hợp luôn trong CÙNG lượt — người dùng thấy cả tin nhắn của cấp
/// dưới lẫn câu tổng hợp của cấp trên xuất hiện cùng lúc, đúng yêu cầu "log lại thành tin nhắn".
pub fn run<FTool, FDelegate, FResolve>(
    input: TurnInput<'_>,
    mut call_tool: FTool,
    mut enqueue_delegation: FDelegate,
    mut resolve_record: FResolve,
) -> Result<TurnOutcome, Error>
where
    FTool: FnMut(&str, &Value) -> Result<Value, Error>,
    FDelegate: FnMut(&str, &str) -> Result<String, Error>,
    FResolve: FnMut(&str, &str) -> Result<Vec<(String, String)>, Error>,
{
    let mut messages = input.messages;
    let mut out = TurnOutcome::default();

    let mut tool_schemas: Vec<Value> = input.tools.iter().map(|(t, _)| t.to_json()).collect();
    if !input.subordinates.is_empty() {
        tool_schemas.push(tools::delegate_schema(&input.subordinates));
    }
    if !input.form_entities.is_empty() {
        tool_schemas.push(tools::form_schema(&input.form_entities));
    }

    for round in 0..MAX_TOOL_ROUNDS {
        let TurnResult { content, tool_calls } = llm::complete(
            input.provider,
            input.model,
            input.system_prompt,
            &messages,
            &tool_schemas,
            input.local_base_url,
        )?;

        let content = strip_form_markup(&content);
        if !content.trim().is_empty() {
            if !out.content.is_empty() {
                out.content.push('\n');
            }
            out.content.push_str(content.trim());
            // Mọi block nằm trong JSON mà model trả kèm văn bản đều phải qua whitelist.
            if let Some(extracted) = extract_blocks(&content) {
                out.blocks.extend(blocks::validate(&extracted, input.allowed_block_types));
            }
        }

        if tool_calls.is_empty() {
            return Ok(out);
        }

        messages.push(LlmMessage::Assistant(if content.trim().is_empty() {
            format!("(gọi {} công cụ)", tool_calls.len())
        } else {
            content.clone()
        }));

        for call in &tool_calls {
            out.tool_calls_made.push(call.name.clone());

            if call.name == tools::DELEGATE_TOOL_NAME {
                let target =
                    call.arguments.get("target_agent").and_then(Value::as_str).unwrap_or_default();
                let task = call
                    .arguments
                    .get("task_description")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                // Chỉ nhận cấp dưới có thật — model có thể bịa tên dù đã khai `enum`.
                if target.is_empty() || !input.subordinates.iter().any(|s| s == target) {
                    messages.push(LlmMessage::Tool {
                        tool_call_id: call.id.clone(),
                        name: call.name.clone(),
                        content: format!(
                            "Không có trợ lý cấp dưới tên '{target}'. Cấp dưới hợp lệ: {}",
                            input.subordinates.join(", ")
                        ),
                    });
                    continue;
                }
                let result_content = enqueue_delegation(target, task)?;
                // Cấp dưới rất hay TRẢ VỀ CÂU HỎI (vd "có 2 nhân viên trùng tên, chọn ai?") — đã đo
                // thật: gặp ca đó model cấp trên lại bịa "đã chỉnh sửa xong" rồi thôi, người dùng
                // tưởng việc đã làm mà thực ra chưa có gì xảy ra. Nói thẳng ranh giới cho nó.
                let result_content = format!(
                    "{result_content}\n\n[Nguyên văn phản hồi của cấp dưới. Nếu trong đó KHÔNG khẳng \
                     định rõ là đã hoàn tất, thì việc CHƯA XONG: chuyển lại đúng ý cấp dưới (câu hỏi, \
                     yêu cầu làm rõ, danh sách để chọn) cho người dùng và CẤM nói là đã xong.]"
                );
                out.delegations.push((target.to_string(), task.to_string()));
                messages.push(LlmMessage::Tool {
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    content: result_content,
                });
                continue;
            }

            if call.name == tools::FORM_TOOL_NAME {
                let entity =
                    call.arguments.get("entity").and_then(Value::as_str).unwrap_or_default();
                // Chỉ nhận entity đã cấp cho trợ lý này — model có thể bịa tên dù đã khai `enum`,
                // và một biểu mẫu trỏ sai entity thì chỉ hiện lỗi đỏ giữa khung chat.
                if entity.is_empty() || !input.form_entities.iter().any(|e| e == entity) {
                    messages.push(LlmMessage::Tool {
                        tool_call_id: call.id.clone(),
                        name: call.name.clone(),
                        content: format!(
                            "Không mở được biểu mẫu cho '{entity}'. Chỉ được mở: {}",
                            input.form_entities.join(", ")
                        ),
                    });
                    continue;
                }
                // Block do CODE sinh (cùng lý do `links`/`confirm`): model không tự vẽ được biểu
                // mẫu, nên không thể bịa ra trường không có thật hay bỏ sót trường bắt buộc —
                // `EntityForm` tự dựng từ định nghĩa entity đã deploy.
                // Người dùng gọi bản ghi bằng TÊN, nên tool cũng chỉ nhận tên/mã — CODE tự tra ra
                // id. Bắt model tự biết id là sai thực tế: đã đo thật, nó hoặc bịa id từ câu hỏi
                // ("… Tiến 2" ⇒ `record_id: "2"`), hoặc quay ra hỏi người dùng đọc UUID cho nó.
                let record = call.arguments.get("record").and_then(Value::as_str).unwrap_or_default().trim();
                let mut record_id = String::new();
                if !record.is_empty() {
                    if looks_like_uuid(record) {
                        record_id = record.to_string();
                    } else {
                        let matches = resolve_record(entity, record)?;
                        match matches.len() {
                            0 => {
                                messages.push(LlmMessage::Tool {
                                    tool_call_id: call.id.clone(),
                                    name: call.name.clone(),
                                    content: format!(
                                        "Không tìm thấy bản ghi nào tên/mã khớp \"{record}\". Hãy tra cứu \
                                         để biết tên chính xác rồi gọi lại, hoặc hỏi lại người dùng."
                                    ),
                                });
                                continue;
                            }
                            1 => record_id = matches[0].0.clone(),
                            _ => {
                                // Nhiều bản ghi khớp ⇒ KHÔNG tự chọn bừa (sửa nhầm hồ sơ người khác
                                // là hỏng dữ liệu thật): đưa danh sách để hỏi lại người dùng.
                                let names: Vec<&str> =
                                    matches.iter().map(|(_, label)| label.as_str()).collect();
                                messages.push(LlmMessage::Tool {
                                    tool_call_id: call.id.clone(),
                                    name: call.name.clone(),
                                    content: format!(
                                        "Có {} bản ghi khớp \"{record}\": {}. Hỏi người dùng muốn sửa \
                                         cái nào rồi gọi lại với tên đầy đủ.",
                                        matches.len(),
                                        names.join(", ")
                                    ),
                                });
                                continue;
                            }
                        }
                    }
                }
                let record_id = record_id.as_str();
                let mut props = json!({ "entity_code": entity });
                if !record_id.is_empty() {
                    props["record_id"] = json!(record_id);
                }
                let block = json!({
                    // Id block phải KHÁC nhau giữa form tạo và form sửa từng bản ghi, nếu không 2
                    // biểu mẫu khác nhau trong cùng 1 lượt bị coi là trùng và bỏ mất 1.
                    "id": format!("form_{entity}_{record_id}"),
                    "type": "EntityForm",
                    "props": props,
                });
                if !out.blocks.contains(&block) {
                    out.blocks.push(block);
                }
                messages.push(LlmMessage::Tool {
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    content: format!(
                        "Đã hiện biểu mẫu {} ngay dưới câu trả lời cho người dùng tự điền và bấm lưu. \
                         Trả lời ĐÚNG 1 CÂU mời họ điền. CẤM liệt kê lại các trường, CẤM hỏi từng \
                         trường, CẤM tự viết thẻ HTML (<form>, <input>, <button>…) — biểu mẫu đã có \
                         sẵn rồi, viết thêm chỉ làm bẩn câu trả lời.",
                        if record_id.is_empty() { "tạo mới" } else { "sửa (đã điền sẵn giá trị hiện tại)" }
                    ),
                });
                continue;
            }

            let Some((schema, openapi_yaml)) =
                input.tools.iter().find(|(schema, _)| schema.name == call.name)
            else {
                // Tool không có trong danh sách được cấp ⇒ từ chối ngay, không thử gọi.
                messages.push(LlmMessage::Tool {
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    content: format!("Trợ lý này không được phép gọi '{}'.", call.name),
                });
                continue;
            };

            match call_tool(&call.name, &call.arguments) {
                Ok(result) => {
                    // Link do CODE sinh từ khai báo, không phải do model (xem `links`).
                    for link in links::extract(openapi_yaml, &result) {
                        let block = link.to_block();
                        if !out.blocks.contains(&block) {
                            out.blocks.push(block);
                        }
                    }
                    // Nút Xác nhận/Huỷ do CODE sinh khi tool trả `awaiting_confirmation: true` —
                    // xem `confirm` (cùng lý do `links`: model không tự vẽ UI). `function_code`
                    // (mã Function, vd "inventory_create_warehouse") KHÁC `call.name` (tên tool =
                    // operationId, vd "executeInventoryCreateWarehouse") — suy ra từ `schema.path`
                    // (`/functions/<code>/execute`), path này do CHÍNH `tools::from_openapi` sinh
                    // nên luôn đúng dạng, không cần validate thêm.
                    let function_code =
                        schema.path.trim_start_matches("/functions/").trim_end_matches("/execute");
                    if let Some(block) = confirm::extract(openapi_yaml, function_code, &result) {
                        if !out.blocks.contains(&block) {
                            out.blocks.push(block);
                        }
                    }
                    messages.push(LlmMessage::Tool {
                        tool_call_id: call.id.clone(),
                        name: call.name.clone(),
                        content: result.to_string(),
                    });
                }
                Err(error) => {
                    // Lỗi tool (vd 403 vì vượt quyền người được gắn) đưa lại cho model để nó giải
                    // thích cho người dùng, thay vì làm sập cả lượt.
                    messages.push(LlmMessage::Tool {
                        tool_call_id: call.id.clone(),
                        name: call.name.clone(),
                        content: format!("Gọi công cụ thất bại: {error:?}"),
                    });
                }
            }
        }

        if round + 1 == MAX_TOOL_ROUNDS {
            out.hit_round_limit = true;
        }
    }

    // Hết vòng mà chưa có chữ nào: model mải gọi tool, vòng cuối lại là tool-call nên không còn
    // lượt nào để nó viết thành lời — người dùng nhận đúng câu "(trợ lý không trả lời được nội dung
    // nào)", vô nghĩa dù việc đã làm xong (đã gặp thật). Hỏi lại ĐÚNG 1 lần, KHÔNG cấp tool nữa, để
    // buộc nó tổng hợp bằng lời những gì vừa làm.
    if out.content.trim().is_empty() {
        if let Ok(TurnResult { content, .. }) = llm::complete(
            input.provider,
            input.model,
            input.system_prompt,
            &messages,
            &[],
            input.local_base_url,
        ) {
            out.content.push_str(content.trim());
        }
    }

    Ok(out)
}

/// Thẻ HTML của biểu mẫu. Model KHÔNG có lý do chính đáng nào để tự viết chúng: biểu mẫu trong chat
/// do CODE sinh (`tools::FORM_TOOL_NAME`), còn khung chat thì render markdown nên thẻ thô chỉ hiện
/// ra dưới dạng chữ `<input type="text" …>` giữa câu trả lời — đã gặp thật.
const FORM_TAGS: &[&str] = &["form", "input", "button", "select", "option", "textarea", "label", "fieldset"];

/// Bỏ mọi đánh dấu biểu mẫu model tự viết trong câu trả lời.
///
/// Cùng lý do tồn tại với `llm::strip_think_blocks`: dặn model trong prompt là không đủ (đã dặn rồi
/// nó vẫn viết), nên chặn bằng code. `<form>` bị bỏ CẢ NỘI DUNG (toàn bộ khối là rác), các thẻ còn
/// lại chỉ bỏ thẻ và giữ chữ bên trong để không mất nội dung thật.
fn strip_form_markup(text: &str) -> String {
    if !text.contains('<') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let closing = after.starts_with('/');
        let name: String = after
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_lowercase();
        if !FORM_TAGS.contains(&name.as_str()) {
            // Dấu `<` thường trong văn bản (vd "a < b") — giữ nguyên, đi tiếp.
            out.push('<');
            rest = after;
            continue;
        }
        if name == "form" && !closing {
            // Cả khối `<form>…</form>` là rác; thẻ mở không đóng ⇒ bỏ hết phần còn lại.
            match after.find("</form>") {
                Some(end) => rest = &after[end + "</form>".len()..],
                None => return out,
            }
            continue;
        }
        match after.find('>') {
            Some(end) => rest = &after[end + 1..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Id bản ghi Picus luôn là UUID. Kiểm tra hình dạng (8-4-4-4-12 hex) đủ để loại các giá trị model
/// tự bịa ra từ câu hỏi; đúng/sai thật sự thì tầng dữ liệu mới biết.
fn looks_like_uuid(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && [8, 4, 4, 4, 12] == groups.iter().map(|g| g.len()).collect::<Vec<_>>()[..]
        && groups.iter().all(|g| g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Lấy mảng `blocks` nếu model trả JSON kèm văn bản.
///
/// Chấp nhận cả nguyên khối JSON `{"content": ..., "blocks": [...]}` và khối ```json trong
/// markdown — model hay bọc JSON trong code fence dù được yêu cầu đừng.
fn extract_blocks(content: &str) -> Option<Value> {
    let trimmed = content.trim();
    let candidate = if let Some(start) = trimmed.find("```json") {
        let rest = &trimmed[start + 7..];
        rest.find("```").map(|end| rest[..end].trim())?
    } else {
        trimmed
    };
    serde_json::from_str::<Value>(candidate).ok()?.get("blocks").cloned()
}

/// Tiện ích cho caller: gộp `content` + `blocks` thành payload lưu vào 1 bản tin.
pub fn message_payload(out: &TurnOutcome) -> Value {
    json!({ "content": out.content, "blocks": out.blocks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_blocks_from_fenced_json() {
        let content = "Đây là báo cáo:\n```json\n{\"content\":\"x\",\"blocks\":[{\"type\":\"Text\"}]}\n```";
        let blocks = extract_blocks(content).expect("phải tìm được blocks");
        assert_eq!(blocks[0]["type"], "Text");
    }

    #[test]
    fn rejects_record_id_model_made_up() {
        assert!(looks_like_uuid("10b6a640-af4c-11f1-8131-f6c2b5074367"));
        assert!(!looks_like_uuid("2"));
        assert!(!looks_like_uuid("nguyên văn tiến 2"));
        assert!(!looks_like_uuid("10b6a640af4c11f18131f6c2b5074367"));
    }

    #[test]
    fn strips_form_markup_model_wrote_itself() {
        let text = "Điền vào đây:\n<form> <input type=\"text\" name=\"a\" required /> <button type=\"submit\">Lưu</button> </form>\nXong.";
        assert_eq!(strip_form_markup(text), "Điền vào đây:\n\nXong.");
    }

    #[test]
    fn strips_stray_form_tags_but_keeps_text() {
        assert_eq!(strip_form_markup("Bấm <button>Lưu</button> nhé"), "Bấm Lưu nhé");
    }

    #[test]
    fn keeps_ordinary_angle_brackets() {
        assert_eq!(strip_form_markup("khi a < b thì <b>đậm</b>"), "khi a < b thì <b>đậm</b>");
    }

    #[test]
    fn plain_text_has_no_blocks() {
        assert!(extract_blocks("chỉ là văn bản thường").is_none());
    }
}
