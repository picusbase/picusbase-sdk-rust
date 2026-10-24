use serde::{Deserialize, Serialize};

// ── Response Types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct OrgUserPool {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub organization_id: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PoolUser {
    pub id: String,
    pub email: String,
    pub status: Option<String>,
    pub role: Option<String>,
    pub email_confirmed_at: Option<String>,
    pub last_sign_in_at: Option<String>,
    pub banned_until: Option<String>,
    pub is_banned: Option<bool>,
    pub user_metadata: Option<serde_json::Value>,
    pub app_metadata: Option<serde_json::Value>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PoolGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub member_count: Option<i64>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PoolProvider {
    pub id: String,
    pub provider_type: String,
    pub display_name: Option<String>,
    pub is_enabled: bool,
    pub client_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PoolClient {
    pub id: String,
    pub user_pool_id: Option<String>,
    pub client_name: String,
    pub client_secret: Option<String>,
    pub redirect_uris: Vec<String>,
    pub token_validity_seconds: Option<i32>,
    pub refresh_token_validity_seconds: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PoolConfig {
    pub redirect_urls: Option<Vec<String>>,
    pub password_policy: Option<serde_json::Value>,
    pub email_templates: Option<serde_json::Value>,
    pub access_token_validity: Option<i32>,
    pub refresh_token_validity: Option<i32>,
}
