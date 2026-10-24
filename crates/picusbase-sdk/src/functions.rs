use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, serde::Deserialize)]
struct ExecuteResponse {
    success: bool,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    result: Option<Value>,
}

// ---------------------------------------------------------------------------
// Surface B — method-aware route-based requests
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum HttpMethod {
    GET,
    #[default]
    POST,
    PUT,
    DELETE,
    PATCH,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::GET => "GET",
            HttpMethod::POST => "POST",
            HttpMethod::PUT => "PUT",
            HttpMethod::DELETE => "DELETE",
            HttpMethod::PATCH => "PATCH",
        }
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub struct FunctionRequest {
    method: HttpMethod,
    path: String,
    query: HashMap<String, String>,
    body: Option<Value>,
}

impl FunctionRequest {
    pub fn get(path: &str) -> Self {
        Self {
            method: HttpMethod::GET,
            path: path.to_string(),
            query: HashMap::new(),
            body: None,
        }
    }

    pub fn post(path: &str) -> Self {
        Self {
            method: HttpMethod::POST,
            path: path.to_string(),
            query: HashMap::new(),
            body: None,
        }
    }

    pub fn put(path: &str) -> Self {
        Self {
            method: HttpMethod::PUT,
            path: path.to_string(),
            query: HashMap::new(),
            body: None,
        }
    }

    pub fn delete(path: &str) -> Self {
        Self {
            method: HttpMethod::DELETE,
            path: path.to_string(),
            query: HashMap::new(),
            body: None,
        }
    }

    pub fn patch(path: &str) -> Self {
        Self {
            method: HttpMethod::PATCH,
            path: path.to_string(),
            query: HashMap::new(),
            body: None,
        }
    }

    pub fn query(mut self, key: &str, value: &str) -> Self {
        self.query.insert(key.to_string(), value.to_string());
        self
    }

    pub fn json<T: Serialize>(mut self, body: &T) -> Result<Self, String> {
        self.body = Some(serde_json::to_value(body).map_err(|e| format!("Serialize error: {e}"))?);
        Ok(self)
    }
}

pub struct FunctionResponse {
    pub status: u16,
    pub body: Value,
}

impl FunctionResponse {
    pub fn json<T: DeserializeOwned>(self) -> Result<T, String> {
        serde_json::from_value(self.body).map_err(|error| format!("Response parse error: {error}"))
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn is_no_content(&self) -> bool {
        self.status == 204
    }
}

pub struct FunctionClient {
    base_url: String,
    auth_token: Option<String>,
    project_ref: Option<String>,
}

impl FunctionClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_token: None,
            project_ref: None,
        }
    }

    pub fn with_auth(mut self, token: Option<&str>) -> Self {
        self.auth_token = token.map(|t| t.to_string());
        self
    }

    pub fn with_project_ref(mut self, project_ref: Option<&str>) -> Self {
        self.project_ref = project_ref.map(|r| r.to_string());
        self
    }

    // Surface A — POST compatibility (execute-by-function-id)
    // -----------------------------------------------------------------

    pub async fn call<T: DeserializeOwned, P: Serialize>(
        &self,
        function_id: &str,
        params: &P,
    ) -> Result<T, String> {
        let result = self.call_raw(function_id, params).await?;
        serde_json::from_value(result).map_err(|error| format!("Response parse error: {error}"))
    }

    pub async fn call_unit<P: Serialize>(
        &self,
        function_id: &str,
        params: &P,
    ) -> Result<(), String> {
        self.call_raw(function_id, params).await.map(|_| ())
    }

    pub async fn call_raw<P: Serialize>(
        &self,
        function_id: &str,
        params: &P,
    ) -> Result<Value, String> {
        let url = format!("{}/functions/{}/execute", self.base_url, function_id);
        let body = serde_json::to_value(params).map_err(|e| format!("Serialize error: {e}"))?;

        #[cfg(target_arch = "wasm32")]
        {
            use gloo_net::http::Request;

            let mut request = Request::post(&url).header("Content-Type", "application/json");

            if let Some(ref token) = self.auth_token {
                request = request.header("Authorization", &format!("Bearer {token}"));
            }
            if let Some(ref project_ref) = self.project_ref {
                request = request.header("x-project-ref", project_ref);
            }

            let response = request
                .json(&body)
                .map_err(|error| format!("Serialize error: {error}"))?
                .send()
                .await
                .map_err(|error| format!("Request failed: {error}"))?;

            if !response.ok() {
                let status = response.status();
                let body_text = response.text().await.unwrap_or_default();
                return Err(format!("HTTP {status}: {body_text}"));
            }

            let payload: Value = response
                .json()
                .await
                .map_err(|error| format!("Response parse error: {error}"))?;

            decode_response_payload(payload)
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let client = reqwest::Client::new();
            let mut request = client.post(&url).json(&body);

            if let Some(ref token) = self.auth_token {
                request = request.header("Authorization", format!("Bearer {token}"));
            }
            if let Some(ref project_ref) = self.project_ref {
                request = request.header("x-project-ref", project_ref.as_str());
            }

            let response = request
                .send()
                .await
                .map_err(|error| format!("Request failed: {error}"))?;

            if !response.status().is_success() {
                let status = response.status();
                let body_text = response.text().await.unwrap_or_default();
                return Err(format!("HTTP {status}: {body_text}"));
            }

            let payload: Value = response
                .json()
                .await
                .map_err(|error| format!("Response parse error: {error}"))?;

            decode_response_payload(payload)
        }
    }

    // -----------------------------------------------------------------
    // Surface B — method-aware route-based requests
    // -----------------------------------------------------------------

    pub async fn request(&self, req: FunctionRequest) -> Result<FunctionResponse, String> {
        let mut url = format!("{}{}", self.base_url, req.path);

        if !req.query.is_empty() {
            let qs = url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(req.query.iter())
                .finish();
            url = format!("{}?{}", url, qs);
        }

        #[cfg(target_arch = "wasm32")]
        {
            use gloo_net::http::Request;

            let mut request = match req.method {
                HttpMethod::GET => Request::get(&url),
                HttpMethod::POST => Request::post(&url),
                HttpMethod::PUT => Request::put(&url),
                HttpMethod::DELETE => Request::delete(&url),
                HttpMethod::PATCH => Request::patch(&url),
            };
            request = request.header("Content-Type", "application/json");

            if let Some(ref token) = self.auth_token {
                request = request.header("Authorization", &format!("Bearer {token}"));
            }
            if let Some(ref project_ref) = self.project_ref {
                request = request.header("x-project-ref", project_ref);
            }

            let response = if let Some(ref body) = req.body {
                request
                    .json(body)
                    .map_err(|error| format!("Serialize error: {error}"))?
                    .send()
                    .await
                    .map_err(|error| format!("Request failed: {error}"))?
            } else {
                request
                    .send()
                    .await
                    .map_err(|error| format!("Request failed: {error}"))?
            };

            let status = response.status();

            if status == 204 {
                return Ok(FunctionResponse {
                    status,
                    body: Value::Null,
                });
            }

            let body: Value = response.json().await.unwrap_or(Value::Null);

            if !(200..300).contains(&status) {
                let msg = body
                    .get("message")
                    .and_then(|v| v.as_str())
                    .or_else(|| body.get("error").and_then(|v| v.as_str()))
                    .unwrap_or("Request failed");
                return Err(format!("HTTP {status}: {msg}"));
            }

            Ok(FunctionResponse { status, body })
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let client = reqwest::Client::new();
            let mut request = match req.method {
                HttpMethod::GET => client.get(&url),
                HttpMethod::POST => client.post(&url),
                HttpMethod::PUT => client.put(&url),
                HttpMethod::DELETE => client.delete(&url),
                HttpMethod::PATCH => client.patch(&url),
            };

            if let Some(ref token) = self.auth_token {
                request = request.header("Authorization", format!("Bearer {token}"));
            }
            if let Some(ref project_ref) = self.project_ref {
                request = request.header("x-project-ref", project_ref.as_str());
            }

            if let Some(ref body) = req.body {
                request = request.json(body);
            }

            let response = request
                .send()
                .await
                .map_err(|error| format!("Request failed: {error}"))?;

            let status = response.status().as_u16();

            if status == 204 {
                return Ok(FunctionResponse {
                    status,
                    body: Value::Null,
                });
            }

            let body: Value = response.json().await.unwrap_or(Value::Null);

            if !(200..300).contains(&status) {
                let msg = body
                    .get("message")
                    .and_then(|v| v.as_str())
                    .or_else(|| body.get("error").and_then(|v| v.as_str()))
                    .unwrap_or("Request failed");
                return Err(format!("HTTP {status}: {msg}"));
            }

            Ok(FunctionResponse { status, body })
        }
    }
}

fn decode_response_payload(value: Value) -> Result<Value, String> {
    if let Ok(payload) = serde_json::from_value::<ExecuteResponse>(value.clone()) {
        if !payload.success {
            return Err(payload
                .message
                .unwrap_or_else(|| "Function execution failed".to_string()));
        }

        return Ok(payload.result.unwrap_or(Value::Null));
    }

    Ok(value)
}
