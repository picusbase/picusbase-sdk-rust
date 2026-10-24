pub mod auth;
#[cfg(not(target_arch = "wasm32"))]
pub mod compute;
pub mod functions;
pub mod meta;
pub mod platform;
pub mod pools;
pub mod realtime;
pub mod storage;
pub mod system;

use auth::AuthClient;
#[cfg(not(target_arch = "wasm32"))]
use compute::ComputeClient;
use picusbase_errors::{ErrorCategory, ErrorLevel, PicusError};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Default, Debug)]
pub struct SessionState {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub client_id: Option<String>,
    pub user: Option<serde_json::Value>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct PicusClient {
    pub auth: Option<AuthClient>,
    pub compute: Option<ComputeClient>,
    pub api_url: Option<String>,
    pub auth_url: Option<String>,
    pub storage_url: Option<String>,
    pub realtime_url: Option<String>,
    pub session: Arc<RwLock<SessionState>>,
    pub headers: HashMap<String, String>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct PicusClient {
    pub auth: Option<AuthClient>,
    pub api_url: Option<String>,
    pub auth_url: Option<String>,
    pub realtime_url: Option<String>,
    pub storage_url: Option<String>,
    pub session: Arc<RwLock<SessionState>>,
    pub headers: HashMap<String, String>,
}

impl PartialEq for PicusClient {
    fn eq(&self, other: &Self) -> bool {
        self.api_url == other.api_url
            && self.auth_url == other.auth_url
            && self.realtime_url == other.realtime_url
            && self.storage_url == other.storage_url
            // Cannot easily compare RwLock contents without locking, skipping for now or assume Arc ptr eq?
            // For now, let's skip session equality or assume if ptrs match.
            && Arc::ptr_eq(&self.session, &other.session)
    }
}

// Internal helper for both
impl PicusClient {
    // Helper to get current token securely
    pub fn token(&self) -> Option<String> {
        self.session.read().ok()?.access_token.clone()
    }

    pub fn refresh_token(&self) -> Option<String> {
        self.session.read().ok()?.refresh_token.clone()
    }

    async fn post_storage_internal(
        &self,
        path: &str,
        body: &serde_json::Value,
        project_ref: Option<&str>,
    ) -> Result<serde_json::Value, PicusError> {
        if let Some(url) = &self.storage_url {
            let target = format!("{}{}", url.trim_end_matches('/'), path);
            let client = reqwest::Client::new();
            let mut req = client.post(&target).json(body);

            if let Some(token) = self.token() {
                req = req.bearer_auth(token);
            }
            if let Some(pr) = project_ref {
                req = req.header("x-project-ref", pr);
            }
            for (k, v) in &self.headers {
                if project_ref.is_some() && k.eq_ignore_ascii_case("x-project-ref") {
                    continue;
                }
                req = req.header(k, v);
            }

            let resp = req.send().await.map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Req error: {}", e),
                )
            })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(PicusError::new(
                    "SDK",
                    ErrorCategory::API,
                    ErrorLevel::Error,
                    status.as_u16(),
                    format!("Error: {}", text),
                ));
            }

            resp.json().await.map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::DATA,
                    ErrorLevel::Error,
                    0,
                    format!("Parse error: {}", e),
                )
            })
        } else {
            Err(PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                "Storage URL not configured".to_string(),
            ))
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) async fn post_storage_multipart(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
        project_ref: Option<&str>,
    ) -> Result<serde_json::Value, PicusError> {
        if let Some(url) = &self.storage_url {
            let target = format!("{}{}", url.trim_end_matches('/'), path);
            let client = reqwest::Client::new();
            let mut req = client.post(&target).multipart(form);

            if let Some(token) = self.token() {
                req = req.bearer_auth(token);
            }
            if let Some(pr) = project_ref {
                req = req.header("x-project-ref", pr);
            }
            for (k, v) in &self.headers {
                if project_ref.is_some() && k.eq_ignore_ascii_case("x-project-ref") {
                    continue;
                }
                req = req.header(k, v);
            }

            let resp = req.send().await.map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Req error: {}", e),
                )
            })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(PicusError::new(
                    "SDK",
                    ErrorCategory::API,
                    ErrorLevel::Error,
                    status.as_u16(),
                    format!("Error: {}", text),
                ));
            }

            resp.json().await.map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::DATA,
                    ErrorLevel::Error,
                    0,
                    format!("Parse error: {}", e),
                )
            })
        } else {
            Err(PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                "Storage URL not configured".to_string(),
            ))
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl PicusClient {
    pub fn init(base_url: &str, api_key: &str, project_ref: Option<&str>) -> Self {
        let base = base_url.trim_end_matches('/');
        let api_url = format!("{}", base);
        let auth_url = format!("{}/api", base);
        let realtime_base = if base.starts_with("https://") {
            format!("wss://{}", base.strip_prefix("https://").unwrap())
        } else if base.starts_with("http://") {
            format!("ws://{}", base.strip_prefix("http://").unwrap())
        } else {
            format!("ws://{}", base)
        };
        let realtime_url = format!("{}/ws", realtime_base);
        let storage_url = format!("{}/storage", base); // Fixed storage URL path

        // If api_key is empty, we treat it as no token
        let token = if api_key.is_empty() {
            None
        } else {
            Some(api_key.to_string())
        };

        let mut headers = HashMap::new();
        if let Some(pr) = project_ref {
            headers.insert("x-project-ref".to_string(), pr.to_string());
        }

        Self::new(
            &api_url,
            &realtime_url,
            Some(&storage_url),
            Some(&auth_url),
            token,
            None,
            None,
            Some(headers),
        )
    }

    pub fn with_project(&self, project_ref: &str) -> Self {
        self.with_header("x-project-ref", project_ref)
    }

    pub fn save_session(
        &self,
        access_token: &str,
        refresh_token: &str,
        user: &serde_json::Value,
        client_id: Option<&str>,
    ) -> Result<Self, String> {
        use gloo_storage::{LocalStorage, Storage};
        LocalStorage::set("access_token", access_token).map_err(|e| e.to_string())?;
        LocalStorage::set("refresh_token", refresh_token).map_err(|e| e.to_string())?;
        LocalStorage::set("user", user).map_err(|e| e.to_string())?;
        if let Some(cid) = client_id {
            LocalStorage::set("client_id", cid).map_err(|e| e.to_string())?;
        }

        // Update internal session state
        if let Ok(mut session) = self.session.write() {
            session.access_token = Some(access_token.to_string());
            session.refresh_token = Some(refresh_token.to_string());
            session.user = Some(user.clone());
            if let Some(cid) = client_id {
                session.client_id = Some(cid.to_string());
            }
        }

        // Return same client since it's shared state (or clone which points to same shared state)
        Ok(self.clone())
    }

    pub async fn refresh_tokens(&self, client_id: &str) -> Result<Self, PicusError> {
        let refresh_token = self
            .session
            .read()
            .ok()
            .and_then(|s| s.refresh_token.clone())
            .ok_or_else(|| {
                PicusError::new(
                    "SDK_AUTH",
                    ErrorCategory::AUTH,
                    ErrorLevel::Error,
                    401,
                    "No refresh token available".to_string(),
                )
            })?;

        let session_data = self
            .auth
            .as_ref()
            .ok_or_else(|| {
                PicusError::new(
                    "SDK_AUTH",
                    ErrorCategory::AUTH,
                    ErrorLevel::Error,
                    0,
                    "Auth client not initialized".to_string(),
                )
            })?
            .refresh_session(&refresh_token, client_id)
            .await?;

        let access = session_data.access_token;
        let refresh = session_data
            .refresh_token
            .as_ref()
            .unwrap_or(&refresh_token)
            .clone();
        let user = session_data.user.unwrap_or(serde_json::json!({}));

        self.save_session(&access, &refresh, &user, Some(client_id))
            .map_err(|e| {
                PicusError::new("SDK_STORAGE", ErrorCategory::DATA, ErrorLevel::Error, 0, e)
            })
    }

    pub fn load_session(&self) -> Self {
        use gloo_storage::{LocalStorage, Storage};

        // Try to load via gloo-storage (expects JSON formatting)
        let mut access_token = LocalStorage::get::<String>("access_token").ok();

        // Fallback: load raw string via web-sys if gloo fails (legacy/manual set)
        if access_token.is_none() {
            #[cfg(target_arch = "wasm32")]
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(raw)) = storage.get_item("access_token") {
                        if !raw.is_empty() && raw != "null" && raw != "undefined" {
                            // If it looks like JSON string (starts with "), remove quotes?
                            // No, gloo would have handled it. This is strictly for raw unquoted.
                            access_token = Some(raw);
                        }
                    }
                }
            }
        }

        if let Some(token) = access_token {
            if !token.is_empty() {
                let mut refresh_token = LocalStorage::get::<String>("refresh_token").ok();

                if refresh_token.is_none() {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(window) = web_sys::window() {
                        if let Ok(Some(storage)) = window.local_storage() {
                            if let Ok(Some(raw)) = storage.get_item("refresh_token") {
                                if !raw.is_empty() {
                                    refresh_token = Some(raw);
                                }
                            }
                        }
                    }
                }

                let mut client_id = LocalStorage::get::<String>("client_id").ok();
                if client_id.is_none() {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(window) = web_sys::window() {
                        if let Ok(Some(storage)) = window.local_storage() {
                            if let Ok(Some(raw)) = storage.get_item("client_id") {
                                if !raw.is_empty() {
                                    client_id = Some(raw);
                                }
                            }
                        }
                    }
                }

                return Self::new(
                    self.api_url.as_deref().unwrap_or(""),
                    self.realtime_url.as_deref().unwrap_or(""),
                    self.storage_url.as_deref(),
                    self.auth_url.as_deref(),
                    Some(token),
                    refresh_token,
                    client_id,
                    Some(self.headers.clone()),
                );
            }
        }
        self.clone()
    }

    pub fn clear_session(&self) -> Result<(), String> {
        use gloo_storage::{LocalStorage, Storage};
        LocalStorage::delete("access_token");
        LocalStorage::delete("refresh_token");
        LocalStorage::delete("user");
        LocalStorage::delete("client_id");

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;

            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.session_storage() {
                    let _ = storage.remove_item("auth_client_id");
                    let _ = storage.remove_item("oauth_callback_code");
                }

                if let Some(document) = window.document() {
                    if let Ok(html_doc) = document.dyn_into::<web_sys::HtmlDocument>() {
                        let _ =
                            html_doc.set_cookie("access_token=; path=/; max-age=0; SameSite=Lax");
                    }
                }
            }
        }

        Ok(())
    }

    pub fn with_token(&self, token: String) -> Self {
        Self::new(
            self.api_url.as_deref().unwrap_or(""),
            self.realtime_url.as_deref().unwrap_or(""),
            self.storage_url.as_deref(),
            self.auth_url.as_deref(),
            Some(token),
            self.refresh_token(),
            self.session.read().ok().and_then(|s| s.client_id.clone()),
            Some(self.headers.clone()),
        )
    }

    pub fn with_session(&self, token: String, refresh_token: Option<String>) -> Self {
        Self::new(
            self.api_url.as_deref().unwrap_or(""),
            self.realtime_url.as_deref().unwrap_or(""),
            self.storage_url.as_deref(),
            self.auth_url.as_deref(),
            Some(token),
            refresh_token,
            self.session.read().ok().and_then(|s| s.client_id.clone()),
            Some(self.headers.clone()),
        )
    }

    pub fn new(
        api_url: &str,
        realtime_url: &str,
        storage_url: Option<&str>,
        auth_url: Option<&str>,
        token: Option<String>,
        refresh_token: Option<String>,
        client_id: Option<String>,
        headers: Option<HashMap<String, String>>,
    ) -> Self {
        let headers = headers.unwrap_or_default();
        let session = Arc::new(RwLock::new(SessionState {
            access_token: token.clone(),
            refresh_token: refresh_token.clone(),
            client_id: client_id,
            user: None,
        }));

        // Build a shared reqwest client with default headers (e.g. x-project-ref)
        let mut header_map = reqwest::header::HeaderMap::new();
        for (k, v) in &headers {
            if let Ok(name) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                if let Ok(val) = reqwest::header::HeaderValue::from_str(v) {
                    header_map.insert(name, val);
                }
            }
        }
        let client = reqwest::Client::builder()
            .default_headers(header_map)
            .build()
            .unwrap_or_default();

        let auth_client =
            auth_url.map(|url| AuthClient::new(url.to_string(), client, session.clone()));

        Self {
            auth: auth_client.clone(),
            realtime_url: Some(realtime_url.to_string()),
            storage_url: Some(storage_url.unwrap_or(api_url).to_string()),
            api_url: Some(api_url.to_string()),
            auth_url: auth_url.map(|s| s.to_string()),
            session,
            headers,
        }
    }

    pub fn with_header(&self, key: &str, value: &str) -> Self {
        let mut new_headers = self.headers.clone();
        new_headers.insert(key.to_string(), value.to_string());

        Self::new(
            self.api_url.as_deref().unwrap_or(""),
            self.realtime_url.as_deref().unwrap_or(""),
            self.storage_url.as_deref(),
            self.auth_url.as_deref(),
            self.token(),
            self.refresh_token(),
            self.session.read().ok().and_then(|s| s.client_id.clone()),
            Some(new_headers),
        )
    }

    pub fn with_schema(&self, schema: &str) -> Self {
        self.with_header("x-schema", schema)
    }

    pub fn realtime(
        &self,
    ) -> Result<
        (
            realtime::wasm::RealtimeSender,
            realtime::wasm::RealtimeReceiver,
        ),
        PicusError,
    > {
        let token_val = self.token().unwrap_or_default();
        let token = token_val.as_str();
        if let Some(url) = &self.realtime_url {
            realtime::wasm::RealtimeClient::connect(url, token)
        } else {
            Err(PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                "Realtime URL not configured".to_string(),
            ))
        }
    }

    pub fn storage(&self) -> storage::client::StorageClient<'_> {
        storage::client::StorageClient {
            client: self,
            project_ref: None,
        }
    }

    pub(crate) async fn post_storage(
        &self,
        path: &str,
        body: &serde_json::Value,
        project_ref: Option<&str>,
    ) -> Result<serde_json::Value, PicusError> {
        self.post_storage_internal(path, body, project_ref).await
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) async fn upload_storage(
        &self,
        path: &str,
        form_data: web_sys::FormData,
        project_ref: Option<&str>,
    ) -> Result<serde_json::Value, PicusError> {
        let storage_url = self.storage_url.as_ref().ok_or_else(|| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                "Storage URL not configured".to_string(),
            )
        })?;

        let url = format!("{}{}", storage_url, path);
        let req = gloo_net::http::Request::post(&url);

        let req = if let Some(token) = self.token() {
            req.header("Authorization", &format!("Bearer {}", token))
        } else {
            req
        };

        let req = if let Some(pr) = project_ref {
            req.header("X-Project-Ref", pr)
        } else {
            req
        };

        let resp = req
            .body(form_data)
            .map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("{:?}", e),
                )
            })?
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("{:?}", e),
                )
            })?;

        if !resp.ok() {
            let status = resp.status();
            let status_text = resp.status_text();

            let error_text = match resp.text().await {
                Ok(text) => text,
                Err(_) => status_text.clone(),
            };

            if let Ok(json_error) = serde_json::from_str::<serde_json::Value>(&error_text) {
                if let Some(error_obj) = json_error.get("error") {
                    if let Ok(picus_error) = serde_json::from_value::<PicusError>(error_obj.clone())
                    {
                        return Err(picus_error);
                    }
                }
            }

            return Err(PicusError::new(
                "SDK",
                ErrorCategory::API,
                ErrorLevel::Error,
                status,
                if !error_text.is_empty() {
                    error_text
                } else {
                    status_text
                },
            ));
        }

        resp.json().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                format!("{:?}", e),
            )
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PicusClient {
    pub fn init(base_url: &str, api_key: &str, project_ref: Option<&str>) -> Self {
        let base = base_url.trim_end_matches('/');
        let api_url = format!("{}", base);
        let auth_url = format!("{}/api", base);
        let realtime_base = if base.starts_with("https://") {
            format!("wss://{}", base.strip_prefix("https://").unwrap())
        } else if base.starts_with("http://") {
            format!("ws://{}", base.strip_prefix("http://").unwrap())
        } else {
            format!("ws://{}", base)
        };
        let realtime_url = format!("{}/ws", realtime_base);
        let storage_url = format!("{}/storage", base);

        let mut headers = HashMap::new();
        if let Some(pr) = project_ref {
            headers.insert("x-project-ref".to_string(), pr.to_string());
        }

        Self::new(
            &api_url,
            &realtime_url,
            Some(&storage_url),
            Some(&auth_url),
            Some(api_key.to_string()),
            None,
            None,
            Some(headers),
        )
    }

    pub fn with_project(&self, project_ref: &str) -> Self {
        self.with_header("x-project-ref", project_ref)
    }

    pub fn with_token(&self, token: String) -> Self {
        Self::new(
            self.api_url.as_deref().unwrap_or(""),
            self.realtime_url.as_deref().unwrap_or(""),
            self.storage_url.as_deref(),
            self.auth_url.as_deref(),
            Some(token),
            self.refresh_token(),
            self.session.read().ok().and_then(|s| s.client_id.clone()),
            Some(self.headers.clone()),
        )
    }

    pub fn with_session(&self, token: String, refresh_token: Option<String>) -> Self {
        Self::new(
            self.api_url.as_deref().unwrap_or(""),
            self.realtime_url.as_deref().unwrap_or(""),
            self.storage_url.as_deref(),
            self.auth_url.as_deref(),
            Some(token),
            refresh_token,
            self.session.read().ok().and_then(|s| s.client_id.clone()),
            Some(self.headers.clone()),
        )
    }

    pub async fn connect(platform_url: &str, _db_url: &str) -> Result<Self, PicusError> {
        let compute = ComputeClient::new(platform_url);
        let session = Arc::new(RwLock::new(SessionState::default()));

        Ok(Self {
            auth: None,
            compute: Some(compute),
            storage_url: None,
            realtime_url: None,
            api_url: None,
            auth_url: None,
            session,
            headers: HashMap::new(),
        })
    }

    pub fn new(
        api_url: &str,
        realtime_url: &str,
        storage_url: Option<&str>,
        auth_url: Option<&str>,
        token: Option<String>,
        refresh_token: Option<String>,
        client_id: Option<String>,
        headers: Option<HashMap<String, String>>,
    ) -> Self {
        let headers = headers.unwrap_or_default();
        let session = Arc::new(RwLock::new(SessionState {
            access_token: token.clone(),
            refresh_token: refresh_token.clone(),
            client_id: client_id,
            user: None,
        }));

        let auth_client = auth_url
            .map(|url| AuthClient::new(url.to_string(), reqwest::Client::new(), session.clone()));

        Self {
            auth: auth_client.clone(),
            compute: None,
            realtime_url: Some(realtime_url.to_string()),
            storage_url: Some(storage_url.unwrap_or(api_url).to_string()),
            api_url: Some(api_url.to_string()),
            auth_url: auth_url.map(|s| s.to_string()),
            session,
            headers,
        }
    }

    pub fn with_header(&self, key: &str, value: &str) -> Self {
        let mut new_headers = self.headers.clone();
        new_headers.insert(key.to_string(), value.to_string());

        Self::new(
            self.api_url.as_deref().unwrap_or(""),
            self.realtime_url.as_deref().unwrap_or(""),
            self.storage_url.as_deref(),
            self.auth_url.as_deref(),
            self.token(),
            self.refresh_token(),
            self.session.read().ok().and_then(|s| s.client_id.clone()),
            Some(new_headers),
        )
    }

    pub fn with_schema(&self, schema: &str) -> Self {
        self.with_header("x-schema", schema)
    }

    /// Backend/native realtime subscription client, scoped to this client's
    /// realtime URL and current access token. Unlike the WASM `realtime()`
    /// (which splits into a sender/receiver pair), this returns a single
    /// `RealtimeClient` — call `.subscribe(topic)` on it directly.
    pub fn realtime(&self) -> Result<realtime::native::RealtimeClient, PicusError> {
        if let Some(url) = &self.realtime_url {
            Ok(realtime::native::RealtimeClient::new(
                url,
                self.token().as_deref(),
            ))
        } else {
            Err(PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                "Realtime URL not configured".to_string(),
            ))
        }
    }

    pub fn storage(&self) -> storage::client::StorageClient<'_> {
        storage::client::StorageClient {
            client: self,
            project_ref: None,
        }
    }

    pub(crate) async fn post_storage(
        &self,
        path: &str,
        body: &serde_json::Value,
        project_ref: Option<&str>,
    ) -> Result<serde_json::Value, PicusError> {
        self.post_storage_internal(path, body, project_ref).await
    }

    /// Persist the session in the OS keychain (desktop apps only — there is no
    /// browser localStorage on native targets). Mirrors the wasm `save_session`
    /// call shape so login/callback pages don't need target-specific code.
    pub fn save_session(
        &self,
        access_token: &str,
        refresh_token: &str,
        user: &serde_json::Value,
        client_id: Option<&str>,
    ) -> Result<Self, String> {
        self.save_session_scoped(DEFAULT_SESSION_SCOPE, access_token, refresh_token, user, client_id)
    }

    /// Same as `save_session`, but under its own keyring entry (`scope`)
    /// instead of the single default one — lets a desktop app that manages
    /// several independent identities at once (e.g. one per picusbase
    /// instance it's logged into, plus a separate cluster/kube-manager
    /// identity) keep each session from clobbering the others.
    pub fn save_session_scoped(
        &self,
        scope: &str,
        access_token: &str,
        refresh_token: &str,
        user: &serde_json::Value,
        client_id: Option<&str>,
    ) -> Result<Self, String> {
        let stored = NativeStoredSession {
            access_token: access_token.to_string(),
            refresh_token: refresh_token.to_string(),
            user: user.clone(),
            client_id: client_id.map(|s| s.to_string()),
        };
        let json = serde_json::to_string(&stored).map_err(|e| e.to_string())?;
        native_keyring_entry_for(scope)?
            .set_password(&json)
            .map_err(|e| e.to_string())?;

        // Build a client around a FRESH `Arc<RwLock<SessionState>>` instead
        // of mutating `self.session` in place. `self.session` is shared by
        // every clone descended from the same PicusClient (cloning only
        // bumps the Arc refcount) — a UI layer holding an older clone in a
        // `Signal<PicusClient>` compares old vs. new by `PartialEq`, which
        // for `session` is `Arc::ptr_eq`. Mutating in place keeps that
        // pointer identical, so the signal write looks like a no-op and
        // subscribers never re-render even though the token changed
        // underneath them — this is what left the desktop app stuck on the
        // login screen after a successful sign-in. A new Arc makes the
        // change visible to `PartialEq`/reactivity as intended.
        let client_id_owned = client_id
            .map(|s| s.to_string())
            .or_else(|| self.session.read().ok().and_then(|s| s.client_id.clone()));

        let new_client = Self::new(
            self.api_url.as_deref().unwrap_or(""),
            self.realtime_url.as_deref().unwrap_or(""),
            self.storage_url.as_deref(),
            self.auth_url.as_deref(),
            Some(access_token.to_string()),
            Some(refresh_token.to_string()),
            client_id_owned,
            Some(self.headers.clone()),
        );
        // `Self::new` always starts `session.user` as None — this Arc is
        // brand new and not yet shared with anything, so writing into it
        // here (before returning) is safe.
        if let Ok(mut session) = new_client.session.write() {
            session.user = Some(user.clone());
        }

        Ok(new_client)
    }

    /// Load a previously persisted session from the OS keychain, if any.
    pub fn load_session(&self) -> Self {
        self.load_session_scoped(DEFAULT_SESSION_SCOPE)
    }

    /// Same as `load_session`, scoped — see `save_session_scoped`.
    pub fn load_session_scoped(&self, scope: &str) -> Self {
        let Ok(entry) = native_keyring_entry_for(scope) else {
            return self.clone();
        };
        let Ok(json) = entry.get_password() else {
            return self.clone();
        };
        let Ok(stored) = serde_json::from_str::<NativeStoredSession>(&json) else {
            return self.clone();
        };
        if stored.access_token.is_empty() {
            return self.clone();
        }

        Self::new(
            self.api_url.as_deref().unwrap_or(""),
            self.realtime_url.as_deref().unwrap_or(""),
            self.storage_url.as_deref(),
            self.auth_url.as_deref(),
            Some(stored.access_token),
            Some(stored.refresh_token),
            stored.client_id,
            Some(self.headers.clone()),
        )
    }

    /// Remove the persisted session from the OS keychain and reset in-memory state.
    pub fn clear_session(&self) -> Result<(), String> {
        self.clear_session_scoped(DEFAULT_SESSION_SCOPE)
    }

    /// Same as `clear_session`, scoped — see `save_session_scoped`.
    pub fn clear_session_scoped(&self, scope: &str) -> Result<(), String> {
        if let Ok(entry) = native_keyring_entry_for(scope) {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        if let Ok(mut session) = self.session.write() {
            *session = SessionState::default();
        }
        Ok(())
    }

    pub async fn refresh_tokens(&self, client_id: &str) -> Result<Self, PicusError> {
        let refresh_token = self
            .session
            .read()
            .ok()
            .and_then(|s| s.refresh_token.clone())
            .ok_or_else(|| {
                PicusError::new(
                    "SDK_AUTH",
                    ErrorCategory::AUTH,
                    ErrorLevel::Error,
                    401,
                    "No refresh token available".to_string(),
                )
            })?;

        let session_data = self
            .auth
            .as_ref()
            .ok_or_else(|| {
                PicusError::new(
                    "SDK_AUTH",
                    ErrorCategory::AUTH,
                    ErrorLevel::Error,
                    0,
                    "Auth client not initialized".to_string(),
                )
            })?
            .refresh_session(&refresh_token, client_id)
            .await?;

        let access = session_data.access_token;
        let refresh = session_data
            .refresh_token
            .as_ref()
            .unwrap_or(&refresh_token)
            .clone();
        let user = session_data.user.unwrap_or(serde_json::json!({}));

        self.save_session(&access, &refresh, &user, Some(client_id))
            .map_err(|e| {
                PicusError::new("SDK_STORAGE", ErrorCategory::DATA, ErrorLevel::Error, 0, e)
            })
    }
}

#[cfg(not(target_arch = "wasm32"))]
const NATIVE_KEYRING_SERVICE: &str = "picus-desktop";

#[cfg(not(target_arch = "wasm32"))]
#[derive(serde::Serialize, serde::Deserialize)]
struct NativeStoredSession {
    access_token: String,
    refresh_token: String,
    user: serde_json::Value,
    client_id: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_SESSION_SCOPE: &str = "session";

// One keychain entry per (OS user account, scope) holds a whole session as
// JSON — simpler than N keyring entries per field, and avoids partial-write
// races between access_token/refresh_token/user living at different keys.
// `scope` lets a caller managing several independent identities at once
// (see `save_session_scoped`) keep each in its own entry instead of one
// session clobbering another's.
#[cfg(not(target_arch = "wasm32"))]
fn native_keyring_entry_for(scope: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(NATIVE_KEYRING_SERVICE, scope).map_err(|e| e.to_string())
}

/// Arbitrary-secret OS keychain storage, independent of `PicusClient`'s own
/// session concept above — for callers that need to remember something
/// smaller/different than a full OAuth session (e.g. a "remember this
/// login form's credentials" checkbox). `scope` is the keychain account
/// name, same namespace as `save_session_scoped`'s — callers should pick
/// scopes that don't collide with a session's own (e.g. prefix with a
/// purpose, not just an instance id).
#[cfg(not(target_arch = "wasm32"))]
pub fn save_secret(scope: &str, value: &str) -> Result<(), String> {
    native_keyring_entry_for(scope)?
        .set_password(value)
        .map_err(|e| e.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_secret(scope: &str) -> Option<String> {
    let entry = match native_keyring_entry_for(scope) {
        Ok(entry) => entry,
        Err(e) => {
            tracing::error!(scope, error = %e, "failed to open OS keychain entry");
            return None;
        }
    };
    match entry.get_password() {
        Ok(password) => Some(password),
        // No entry yet is the normal "nothing saved under this scope" case,
        // not a failure — everything else (locked keychain, access denied
        // for this build's code signature, etc.) used to be silently
        // swallowed the same way, which made "the OS refused this read" and
        // "nothing was ever saved" indistinguishable from the caller's side.
        Err(keyring::Error::NoEntry) => None,
        Err(e) => {
            tracing::error!(scope, error = %e, "failed to read OS keychain entry");
            None
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn clear_secret(scope: &str) -> Result<(), String> {
    match native_keyring_entry_for(scope)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
