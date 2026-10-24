use picusbase_errors::{ErrorCategory, ErrorLevel, PicusError};
use serde::{Deserialize, Serialize};

// ── Response Types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ServiceStatus {
    pub name: String,
    pub status: String,
    pub latency_ms: Option<u64>,
    pub details: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ClusterHealth {
    pub services: Vec<ServiceStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DbSize {
    pub formatted: String,
    pub size_bytes: i64,
    pub database_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TableSize {
    pub table_name: String,
    pub total_size: String,
    pub size_bytes: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SystemHealthStats {
    pub total_connections: i64,
    pub active_connections: i64,
    pub idle_connections: i64,
    pub waiting_connections: i64,
    pub max_connections: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SystemVersion {
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SystemQuery {
    pub query_id: String,
    pub query_text: String,
    pub calls: i64,
    pub mean_exec_time: f64,
    pub total_exec_time: f64,
}

// ── Request Metrics Types ────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ServiceStats {
    pub service_name: String,
    pub request_count: i64,
    pub avg_latency_ms: f64,
    pub error_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RequestStats {
    pub total_requests: i64,
    pub avg_latency_ms: f64,
    pub error_rate: f64,
    pub services: Vec<ServiceStats>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ChartDataPoint {
    pub time: String,
    pub value: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RequestCharts {
    pub request_volume: Vec<ChartDataPoint>,
    pub latency: Vec<ChartDataPoint>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TopClient {
    pub client_ip: String,
    pub request_count: i64,
    pub top_service: String,
    pub last_seen: String,
}

// ── Client ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SystemClient {
    base_url: String,
    client: reqwest::Client,
}

impl SystemClient {
    /// Every `/system/*` operation needs a superuser Bearer token: `client` must send it as a
    /// default `Authorization` header (the client adds no credentials itself).
    pub fn new(base_url: String, client: reqwest::Client) -> Self {
        Self { base_url, client }
    }

    fn map_err(&self, e: reqwest::Error) -> PicusError {
        PicusError::new(
            "SDK",
            ErrorCategory::NET,
            ErrorLevel::Error,
            1,
            format!("Network error: {}", e),
        )
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, PicusError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| self.map_err(e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK",
                ErrorCategory::API,
                ErrorLevel::Error,
                status.as_u16(),
                format!("API Error {}: {}", status, text),
            ));
        }

        resp.json::<T>().await.map_err(|e| self.map_err(e))
    }

    pub async fn get_db_health(&self) -> Result<SystemHealthStats, PicusError> {
        self.get("/system/admin/db-health").await
    }

    pub async fn get_db_size(&self) -> Result<DbSize, PicusError> {
        self.get("/system/admin/db-size").await
    }

    pub async fn get_cluster_size(&self) -> Result<DbSize, PicusError> {
        self.get("/system/admin/cluster-size").await
    }

    pub async fn get_system_version(&self) -> Result<SystemVersion, PicusError> {
        self.get("/system/admin/version").await
    }

    pub async fn get_table_sizes(&self, project_ref: &str) -> Result<Vec<TableSize>, PicusError> {
        let url = format!(
            "{}/system/admin/table-sizes?project_ref={}",
            self.base_url, project_ref
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| self.map_err(e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK",
                ErrorCategory::API,
                ErrorLevel::Error,
                status.as_u16(),
                format!("API Error {}: {}", status, text),
            ));
        }

        resp.json().await.map_err(|e| self.map_err(e))
    }

    pub async fn get_slow_queries(&self) -> Result<Vec<SystemQuery>, PicusError> {
        self.get("/system/admin/slow-queries").await
    }

    pub async fn get_service_health(&self) -> Result<ClusterHealth, PicusError> {
        self.get("/system/admin/service-health").await
    }

    // ── Request Metrics ──────────────────────────────────────────────────────

    pub async fn get_request_stats(&self, range: &str) -> Result<RequestStats, PicusError> {
        let url = format!("{}/system/request-stats?range={}", self.base_url, range);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| self.map_err(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK",
                ErrorCategory::API,
                ErrorLevel::Error,
                status.as_u16(),
                format!("API Error {}: {}", status, text),
            ));
        }
        resp.json().await.map_err(|e| self.map_err(e))
    }

    pub async fn get_request_charts(
        &self,
        range: &str,
        service: Option<&str>,
    ) -> Result<RequestCharts, PicusError> {
        let mut url = format!("{}/system/request-charts?range={}", self.base_url, range);
        if let Some(svc) = service {
            url.push_str(&format!("&service={}", svc));
        }
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| self.map_err(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK",
                ErrorCategory::API,
                ErrorLevel::Error,
                status.as_u16(),
                format!("API Error {}: {}", status, text),
            ));
        }
        resp.json().await.map_err(|e| self.map_err(e))
    }

    pub async fn get_top_clients(
        &self,
        range: &str,
        limit: u32,
    ) -> Result<Vec<TopClient>, PicusError> {
        let url = format!(
            "{}/system/top-clients?range={}&limit={}",
            self.base_url, range, limit
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| self.map_err(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(PicusError::new(
                "SDK",
                ErrorCategory::API,
                ErrorLevel::Error,
                status.as_u16(),
                format!("API Error {}: {}", status, text),
            ));
        }
        resp.json().await.map_err(|e| self.map_err(e))
    }
}
