use crate::SessionState;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use futures::lock::Mutex;
use picusbase_errors::{ErrorCategory, ErrorLevel, PicusError};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, RwLock};
use url::Url;

#[derive(Clone)]
pub struct AuthClient {
    base_url: String,
    client: reqwest::Client,
    session: Arc<RwLock<SessionState>>,
    refresh_lock: Arc<Mutex<()>>,
}

#[derive(Serialize)]
struct ExchangeRequest {
    target_project_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_audience: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requested_scopes: Option<Vec<String>>,
}

#[derive(Serialize)]
struct SignupRequest {
    email: String,
    password: String,
    full_name: Option<String>,
}

#[derive(Serialize)]
struct TokenExchangeRequest {
    grant_type: String,
    code: String,
    redirect_uri: String,
    client_id: String,
    code_verifier: String,
}

#[derive(Serialize)]
struct RefreshTokenRequest {
    grant_type: String,
    refresh_token: String,
    client_id: String,
}

#[derive(Serialize)]
struct ClientCredentialsRequest {
    grant_type: String,
    client_id: String,
    client_secret: String,
    target_project_ref: String,
    audience: String,
    scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_capability: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delegated_actor_user_id: Option<String>,
}

#[derive(Deserialize)]
pub struct ExchangeResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub project_ref: Option<String>,
    pub audience: Option<String>,
    pub scope: Option<String>,
}

#[derive(Deserialize)]
pub struct AuthSession {
    pub access_token: String,
    pub token_type: Option<String>,
    pub expires_in: Option<i64>,
    pub refresh_token: Option<String>,
    pub user: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug)]
pub struct User {
    pub id: String,
    pub email: String,
    pub created_at: String,
    pub updated_at: Option<String>,
    #[serde(alias = "raw_app_meta_data")]
    pub app_metadata: Option<serde_json::Value>,
    #[serde(alias = "raw_user_meta_data")]
    pub user_metadata: Option<serde_json::Value>,
}

impl AuthClient {
    pub fn new(
        base_url: String,
        client: reqwest::Client,
        session: Arc<RwLock<SessionState>>,
    ) -> Self {
        Self {
            base_url,
            client,
            session,
            refresh_lock: Arc::new(Mutex::new(())),
        }
    }

    fn token(&self) -> Option<String> {
        self.session.read().ok()?.access_token.clone()
    }

    fn store_session(&self, auth_session: &AuthSession, client_id: &str) {
        if let Ok(mut session) = self.session.write() {
            session.access_token = Some(auth_session.access_token.clone());
            if let Some(refresh_token) = &auth_session.refresh_token {
                session.refresh_token = Some(refresh_token.clone());
            }
            if let Some(user) = &auth_session.user {
                session.user = Some(user.clone());
            }
            if !client_id.is_empty() {
                session.client_id = Some(client_id.to_string());
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            use gloo_storage::{LocalStorage, Storage};

            let _ = LocalStorage::set("access_token", &auth_session.access_token);
            if let Some(refresh_token) = &auth_session.refresh_token {
                let _ = LocalStorage::set("refresh_token", refresh_token);
            }
            if let Some(user) = &auth_session.user {
                let _ = LocalStorage::set("user", user);
            }
            if !client_id.is_empty() {
                let _ = LocalStorage::set("client_id", client_id);
            }
        }
    }

    /// Sign up a new user
    pub async fn sign_up(
        &self,
        email: &str,
        password: &str,
        full_name: Option<String>,
    ) -> Result<AuthSession, PicusError> {
        let url = format!("{}/auth/signup", self.base_url);
        let mut req = self.client.post(&url).json(&SignupRequest {
            email: email.to_string(),
            password: password.to_string(),
            full_name,
        });

        if let Some(token) = self.token() {
            req = req.bearer_auth(token);
        }

        let resp = req.send().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                format!("Failed to send signup request: {}", e),
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK_AUTH",
                ErrorCategory::AUTH,
                ErrorLevel::Error,
                status.as_u16(),
                format!("Signup failed: {} - {}", status, text),
            ));
        }

        resp.json::<AuthSession>().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                format!("Failed to parse signup response: {}", e),
            )
        })
    }

    pub async fn sign_in_with_password(
        &self,
        email: &str,
        password: &str,
        client_id: &str,
    ) -> Result<AuthSession, PicusError> {
        // 1. Generate PKCE params
        let verifier = Self::generate_pkce_verifier();
        let challenge = Self::generate_pkce_challenge(&verifier);
        let state = Self::generate_pkce_verifier();
        let redirect_uri = self.derive_oauth_callback_uri()?;

        let url = format!("{}/auth/oauth/login", self.base_url);

        #[derive(Serialize)]
        struct OAuthLoginPayload<'a> {
            email: &'a str,
            password: &'a str,
            client_id: &'a str,
            redirect_uri: &'a str,
            code_challenge: String,
            code_challenge_method: &'a str,
            state: Option<&'a str>,
        }

        let payload = OAuthLoginPayload {
            email,
            password,
            client_id,
            redirect_uri: &redirect_uri,
            code_challenge: challenge,
            code_challenge_method: "S256",
            state: Some(&state),
        };

        // 2. Post Credentials
        // Create a new client to handle redirect manually (policy: none)
        // On WASM, we cannot control redirect policy (fetch follows automatically).
        #[cfg(not(target_arch = "wasm32"))]
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    e.to_string(),
                )
            })?;

        #[cfg(target_arch = "wasm32")]
        let client = reqwest::Client::builder().build().map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                e.to_string(),
            )
        })?;

        // Build request:
        // Note: We intentionally use .form() here because `login_post_handler` expects Form<LoginPayload>
        let req = client.post(&url);

        // Add bearer if present (SDK auth)
        let req = if let Some(token) = self.token() {
            req.bearer_auth(token)
        } else {
            req
        };

        let resp = req.form(&payload).send().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                format!("Login req failed: {}", e),
            )
        })?;

        // 3. Check for Redirect (303 See Other is typical for Post->Redirect)
        if resp.status().is_redirection() {
            let location = resp.headers().get("location").ok_or_else(|| {
                PicusError::new(
                    "SDK_AUTH",
                    ErrorCategory::AUTH,
                    ErrorLevel::Error,
                    0,
                    "No location header".to_string(),
                )
            })?;

            let location_str = location.to_str().map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::DATA,
                    ErrorLevel::Error,
                    0,
                    e.to_string(),
                )
            })?;

            // Parse URL
            let redirect_url = Url::parse(location_str).map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::DATA,
                    ErrorLevel::Error,
                    0,
                    format!("Invalid redirect URL: {}", e),
                )
            })?;

            let code = Self::extract_authorization_code(&redirect_url, &state)?;

            // 4. Exchange Code for Session
            self.exchange_code_for_session(&code, client_id, &redirect_uri, &verifier)
                .await
        } else if resp.status().is_success() {
            #[cfg(target_arch = "wasm32")]
            {
                // On WASM, fetch follows redirects automatically.
                // So a 200 OK means we likely landed on the callback URL.
                let final_url = resp.url();
                let code = Self::extract_authorization_code(final_url, &state)?;

                self.exchange_code_for_session(&code, client_id, &redirect_uri, &verifier)
                    .await
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                // If 200 OK (e.g. HTML returned because of error), treat as failure
                Err(PicusError::new(
                    "SDK_AUTH",
                    ErrorCategory::AUTH,
                    ErrorLevel::Error,
                    0,
                    "Login failed (returned HTML instead of redirect)".to_string(),
                ))
            }
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            Err(PicusError::new(
                "SDK_AUTH",
                ErrorCategory::AUTH,
                ErrorLevel::Error,
                status.as_u16(),
                format!("Login failed: {}", text),
            ))
        }
    }

    /// Sign out (Invalidate Session)
    pub async fn sign_out(&self) -> Result<(), PicusError> {
        let url = format!("{}/auth/logout", self.base_url);
        let mut req = self.client.post(&url);

        if let Some(token) = self.token() {
            req = req.bearer_auth(token);
        }

        let resp = req.send().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                format!("Failed to send logout request: {}", e),
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK_AUTH",
                ErrorCategory::AUTH,
                ErrorLevel::Error,
                status.as_u16(),
                format!("Logout failed: {} - {}", status, text),
            ));
        }

        Ok(())
    }

    /// Exchange a System Token for a Project Token
    pub async fn exchange_token(&self, project_ref: &str) -> Result<ExchangeResponse, PicusError> {
        self.exchange_token_for_audience(project_ref, None).await
    }

    /// Exchange a token for a project-scoped token with an optional target audience.
    pub async fn exchange_token_for_audience(
        &self,
        project_ref: &str,
        target_audience: Option<&str>,
    ) -> Result<ExchangeResponse, PicusError> {
        self.exchange_token_with_scopes(project_ref, target_audience, None)
            .await
    }

    /// Exchange a token for a project-scoped token with optional target audience and scopes.
    pub async fn exchange_token_with_scopes(
        &self,
        project_ref: &str,
        target_audience: Option<&str>,
        requested_scopes: Option<Vec<String>>,
    ) -> Result<ExchangeResponse, PicusError> {
        let url = format!("{}/auth/token/exchange", self.base_url);

        let mut req = self.client.post(&url).json(&ExchangeRequest {
            target_project_ref: project_ref.to_string(),
            target_audience: target_audience.map(str::to_string),
            requested_scopes,
        });

        if let Some(token) = self.token() {
            req = req.bearer_auth(token);
        }

        let resp = req.send().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                format!("Failed to send request: {}", e),
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK_AUTH",
                ErrorCategory::AUTH,
                ErrorLevel::Error,
                0,
                format!("Token exchange failed: status={} body={}", status, text),
            ));
        }

        let data = resp.json::<ExchangeResponse>().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                format!("Failed to parse response: {}", e),
            )
        })?;

        Ok(data)
    }

    // --- OAuth 2.0 PKCE Methods ---

    /// Generate a random PKCE Code Verifier (High entropy string)
    pub fn generate_pkce_verifier() -> String {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Generate PKCE Code Challenge (Base64Url(SHA256(verifier)))
    pub fn generate_pkce_challenge(verifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let result = hasher.finalize();
        URL_SAFE_NO_PAD.encode(result)
    }

    /// Construct the Authorization URL to redirect the user to
    pub fn get_authorization_url(
        &self,
        client_id: &str,
        redirect_uri: &str,
        scope: Option<&str>,
        code_challenge: &str,
        state: &str,
    ) -> String {
        let mut url = Url::parse(&format!("{}/auth/oauth/authorize", self.base_url))
            .expect("Invalid base URL configuration");

        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", scope.unwrap_or("openid profile email"))
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state);

        url.to_string()
    }

    fn derive_oauth_callback_uri(&self) -> Result<String, PicusError> {
        let mut url = Url::parse(&self.base_url).map_err(|e| {
            PicusError::new(
                "SDK_AUTH",
                ErrorCategory::AUTH,
                ErrorLevel::Error,
                0,
                format!("Invalid auth base URL: {}", e),
            )
        })?;

        url.set_path("/callback");
        url.set_query(None);
        url.set_fragment(None);

        Ok(url.to_string())
    }

    fn extract_authorization_code(
        redirect_url: &Url,
        expected_state: &str,
    ) -> Result<String, PicusError> {
        let code = redirect_url
            .query_pairs()
            .find(|(key, _)| key == "code")
            .map(|(_, value)| value.to_string())
            .ok_or_else(|| {
                PicusError::new(
                    "SDK_AUTH",
                    ErrorCategory::AUTH,
                    ErrorLevel::Error,
                    0,
                    "No code in redirect URL".to_string(),
                )
            })?;

        let state = redirect_url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.to_string())
            .ok_or_else(|| {
                PicusError::new(
                    "SDK_AUTH",
                    ErrorCategory::AUTH,
                    ErrorLevel::Error,
                    0,
                    "No state in redirect URL".to_string(),
                )
            })?;

        if state != expected_state {
            return Err(PicusError::new(
                "SDK_AUTH",
                ErrorCategory::AUTH,
                ErrorLevel::Error,
                0,
                "State mismatch in redirect URL".to_string(),
            ));
        }

        Ok(code)
    }

    /// Exchange Authorization Code for Session (Tokens)
    pub async fn exchange_code_for_session(
        &self,
        code: &str,
        client_id: &str,
        redirect_uri: &str,
        code_verifier: &str,
    ) -> Result<AuthSession, PicusError> {
        let url = format!("{}/auth/oauth/token", self.base_url);
        let req_body = TokenExchangeRequest {
            grant_type: "authorization_code".to_string(),
            code: code.to_string(),
            client_id: client_id.to_string(),
            redirect_uri: redirect_uri.to_string(),
            code_verifier: code_verifier.to_string(),
        };

        let resp = self
            .client
            .post(&url)
            .json(&req_body)
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to send token exchange request: {}", e),
                )
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK_AUTH",
                ErrorCategory::AUTH,
                ErrorLevel::Error,
                status.as_u16(),
                format!("Token exchange failed: {} - {}", status, text),
            ));
        }

        let auth_session = resp.json::<AuthSession>().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                format!("Failed to parse token response: {}", e),
            )
        })?;

        self.store_session(&auth_session, client_id);

        Ok(auth_session)
    }

    /// Refresh the session using a refresh token
    pub async fn refresh_session(
        &self,
        refresh_token: &str,
        client_id: &str,
    ) -> Result<AuthSession, PicusError> {
        let _refresh_guard = self.refresh_lock.lock().await;

        let url = format!("{}/auth/oauth/token", self.base_url);
        let req_body = RefreshTokenRequest {
            grant_type: "refresh_token".to_string(),
            refresh_token: refresh_token.to_string(),
            client_id: client_id.to_string(),
        };

        let resp = self
            .client
            .post(&url)
            .json(&req_body)
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to send token refresh request: {}", e),
                )
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK_AUTH",
                ErrorCategory::AUTH,
                ErrorLevel::Error,
                status.as_u16(),
                format!("Token refresh failed: {} - {}", status, text),
            ));
        }

        let auth_session = resp.json::<AuthSession>().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                format!("Failed to parse token response: {}", e),
            )
        })?;

        self.store_session(&auth_session, client_id);

        Ok(auth_session)
    }

    /// Exchange service-principal credentials for a short-lived machine token.
    pub async fn client_credentials(
        &self,
        client_id: &str,
        client_secret: &str,
        target_project_ref: &str,
        audience: &str,
        scopes: Vec<String>,
    ) -> Result<AuthSession, PicusError> {
        let url = format!("{}/auth/oauth/token", self.base_url);
        let req_body = ClientCredentialsRequest {
            grant_type: "client_credentials".to_string(),
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            target_project_ref: target_project_ref.to_string(),
            audience: audience.to_string(),
            scope: scopes.join(" "),
            agent_capability: None,
            delegated_actor_user_id: None,
        };

        let resp = self
            .client
            .post(&url)
            .json(&req_body)
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to send client_credentials request: {}", e),
                )
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK_AUTH",
                ErrorCategory::AUTH,
                ErrorLevel::Error,
                status.as_u16(),
                format!("Client credentials grant failed: {} - {}", status, text),
            ));
        }

        resp.json::<AuthSession>().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                format!("Failed to parse client_credentials response: {}", e),
            )
        })
    }

    /// Exchange agent-principal credentials for a short-lived agent token.
    pub async fn agent_client_credentials(
        &self,
        client_id: &str,
        client_secret: &str,
        target_project_ref: &str,
        audience: &str,
        scopes: Vec<String>,
        agent_capability: &str,
        delegated_actor_user_id: Option<&str>,
    ) -> Result<AuthSession, PicusError> {
        let url = format!("{}/auth/oauth/token", self.base_url);
        let req_body = ClientCredentialsRequest {
            grant_type: "client_credentials".to_string(),
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            target_project_ref: target_project_ref.to_string(),
            audience: audience.to_string(),
            scope: scopes.join(" "),
            agent_capability: Some(agent_capability.to_string()),
            delegated_actor_user_id: delegated_actor_user_id.map(str::to_string),
        };

        let resp = self
            .client
            .post(&url)
            .json(&req_body)
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    0,
                    format!("Failed to send agent client_credentials request: {}", e),
                )
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK_AUTH",
                ErrorCategory::AUTH,
                ErrorLevel::Error,
                status.as_u16(),
                format!(
                    "Agent client credentials grant failed: {} - {}",
                    status, text
                ),
            ));
        }

        resp.json::<AuthSession>().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                format!("Failed to parse agent client_credentials response: {}", e),
            )
        })
    }
}
