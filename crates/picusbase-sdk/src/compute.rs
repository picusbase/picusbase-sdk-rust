use picus_errors::{ErrorCategory, ErrorLevel, PicusError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone)]
pub struct ComputeClient {
    base_url: String,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: i64,
    pub level: String,
    pub message: String,
    pub metadata: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionStats {
    pub total_invocations: u64,
    pub avg_duration: f64,
    pub error_count: u64,
    pub last_execution: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChartDataPoint {
    pub time: String,
    pub value: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionCharts {
    pub invocations: Vec<ChartDataPoint>,
    pub duration: Vec<ChartDataPoint>,
    pub errors: Vec<ChartDataPoint>,
}

impl PartialEq for ComputeClient {
    fn eq(&self, other: &Self) -> bool {
        self.base_url == other.base_url
    }
}

impl ComputeClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Execute a function synchronously (via HTTP)
    pub async fn call_function<T: Serialize>(
        &self,
        function_id: &str,
        params: T,
    ) -> Result<Value, PicusError> {
        let url = format!("{}/functions/{}/execute", self.base_url, function_id);

        let response = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "params": params }))
            .send()
            .await
            .map_err(|e| {
                PicusError::new(
                    "SDK",
                    ErrorCategory::API,
                    ErrorLevel::Error,
                    2,
                    format!("Failed to call function: {}", e),
                )
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK",
                ErrorCategory::API,
                ErrorLevel::Error,
                3,
                format!("Function execution failed ({}): {}", status, text),
            ));
        }

        let result: Value = response.json().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::API,
                ErrorLevel::Error,
                4,
                format!("Failed to parse response: {}", e),
            )
        })?;

        Ok(result)
    }

    pub async fn get_logs(
        &self,
        function_id: &str,
        limit: Option<u64>,
    ) -> Result<Vec<LogEntry>, PicusError> {
        let url = format!("{}/functions/{}/logs", self.base_url, function_id);
        let mut req = self.client.get(&url);
        if let Some(l) = limit {
            req = req.query(&[("limit", l)]);
        }

        let response = req.send().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                e.to_string(),
            )
        })?;

        if !response.status().is_success() {
            return Err(PicusError::new(
                "SDK",
                ErrorCategory::API,
                ErrorLevel::Error,
                response.status().as_u16(),
                "Failed to fetch logs".to_string(),
            ));
        }

        response.json().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                e.to_string(),
            )
        })
    }

    pub async fn get_stats(&self, function_id: &str) -> Result<FunctionStats, PicusError> {
        let url = format!("{}/functions/{}/stats", self.base_url, function_id);
        let response = self.client.get(&url).send().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                e.to_string(),
            )
        })?;

        if !response.status().is_success() {
            return Err(PicusError::new(
                "SDK",
                ErrorCategory::API,
                ErrorLevel::Error,
                response.status().as_u16(),
                "Failed to fetch stats".to_string(),
            ));
        }

        response.json().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                e.to_string(),
            )
        })
    }

    pub async fn get_charts(
        &self,
        function_id: &str,
        range: Option<String>,
    ) -> Result<FunctionCharts, PicusError> {
        let url = format!("{}/functions/{}/charts", self.base_url, function_id);
        let mut req = self.client.get(&url);
        if let Some(r) = range {
            req = req.query(&[("range", r)]);
        }
        let response = req.send().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::NET,
                ErrorLevel::Error,
                0,
                e.to_string(),
            )
        })?;

        if !response.status().is_success() {
            return Err(PicusError::new(
                "SDK",
                ErrorCategory::API,
                ErrorLevel::Error,
                response.status().as_u16(),
                "Failed to fetch charts".to_string(),
            ));
        }

        response.json().await.map_err(|e| {
            PicusError::new(
                "SDK",
                ErrorCategory::DATA,
                ErrorLevel::Error,
                0,
                e.to_string(),
            )
        })
    }
}
