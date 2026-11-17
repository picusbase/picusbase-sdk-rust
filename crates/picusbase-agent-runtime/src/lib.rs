//! `picusbase-agent-runtime` — lõi điều phối cho trợ lý AI, dùng chung cho MỌI app chạy trên
//! picusbase (không riêng `picus-erp`).
//!
//! # Ranh giới: crate = cơ chế, Function = chính sách/schema
//!
//! Crate này **không biết** tên bảng (`entity_ai_agent`, `entity_ai_message`, …). Function của
//! từng app tự đọc/ghi bảng của mình rồi gọi [`turn::run`] ở giữa. Nhờ vậy app thứ hai muốn có
//! trợ lý AI chỉ cần viết 1 lớp mỏng, không chép lại vòng lặp.
//!
//! # Chạy bên trong guest sandbox
//!
//! Mọi lệnh gọi mạng đi qua `picusbase_function_sdk::host::http_request` (capability `http.fetch`) —
//! KHÔNG tự mở socket. Đây là lý do không dùng được các crate client LLM phổ thông (rig, genai,
//! async-openai): chúng tự mở kết nối HTTP/TLS riêng, vừa không chạy được trong sandbox vừa vượt
//! qua `PICUSBASE_FETCH_ALLOWLIST` và phần đếm capability của mediator.
//!
//! # Khoá API không nằm ở đây
//!
//! Crate **không nhận, không đọc, không truyền** khoá LLM. `compute-mediator` giữ khoá và tự chèn
//! header xác thực theo host đích — cùng nguyên tắc với `db.query.*`: function truy vấn được DB mà
//! không bao giờ thấy mật khẩu DB.

pub mod blocks;
pub mod confirm;
pub mod links;
pub mod llm;
pub mod tools;
pub mod turn;

pub use blocks::validate as validate_blocks;
pub use llm::{complete, LlmMessage, LlmProvider, ToolCall, TurnResult};
pub use tools::{delegate_schema, from_openapi, ToolSchema, DELEGATE_TOOL_NAME};
pub use turn::{run, TurnInput, TurnOutcome};
