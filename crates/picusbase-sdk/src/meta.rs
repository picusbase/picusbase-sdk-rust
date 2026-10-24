use picus_errors::{ErrorCategory, ErrorLevel, PicusError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone)]
pub struct MetaClient {
    base_url: String,
    client: reqwest::Client,
    token: Option<String>,
}

#[derive(Serialize)]
struct SqlRequest {
    sql: String,
    params: Option<Vec<Value>>,
    /// Target database for the `system` pseudo-project's admin views (every other project is one
    /// database already, so the server ignores this) — see console-api `resolve_database_pool`.
    #[serde(skip_serializing_if = "Option::is_none")]
    database: Option<String>,
}

// --- Types ---

#[derive(Debug, Serialize, Deserialize)]
pub struct Schema {
    pub id: i64,
    pub name: String,
    pub owner: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Table {
    pub id: i64,
    pub schema: String,
    pub name: String,
    pub kind: String,
    pub comment: Option<String>,
    pub rows_estimated: Option<i64>,
    pub size_estimated: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Column {
    pub id: String,
    pub table_id: i64,
    pub schema: String,
    pub table: String,
    pub name: String,
    pub ordinal_position: i32,
    pub default_value: Option<String>,
    pub is_nullable: bool,
    pub data_type: String,
    pub format: String,
    pub is_identity: bool,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Function {
    pub id: i64,
    pub schema: String,
    pub name: String,
    pub language: String,
    pub return_type: String,
    pub argument_types: String,
    pub volatility: String,
    pub security_definer: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Trigger {
    pub id: i64,
    pub schema: String,
    pub table: String,
    pub name: String,
    pub event_manipulation: String,
    pub action_timing: String,
    pub action_statement: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseType {
    pub id: i64,
    pub schema: String,
    pub name: String,
    pub kind: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Extension {
    pub name: String,
    pub version: String,
    pub schema: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Publication {
    pub id: i64,
    pub name: String,
    pub owner: String,
    pub insert: bool,
    pub update: bool,
    pub delete: bool,
    pub truncate: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Index {
    pub id: i64,
    pub schema: String,
    pub table: String,
    pub name: String,
    pub definition: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemLoad {
    pub active: Option<i64>,
    pub idle: Option<i64>,
    pub waiting: Option<i64>,
    pub max_connections: Option<i32>,
}

// --- Requests ---

#[derive(Debug, Serialize)]
pub struct CreateSchemaRequest {
    pub name: String,
    pub owner: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AlterSchemaRequest {
    pub name: Option<String>,
    pub owner: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateTableRequest {
    pub name: String,
    pub schema: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AlterTableRequest {
    pub name: Option<String>,
    pub schema: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateColumnRequest {
    pub name: String,
    pub data_type: String,
    pub default_value: Option<String>,
    pub is_nullable: Option<bool>,
    pub is_unique: Option<bool>,
    pub is_primary_key: Option<bool>,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlterColumnRequest {
    pub name: Option<String>,
    pub data_type: Option<String>,
    pub default_value: Option<String>,
    pub is_nullable: Option<bool>,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Policy {
    pub id: i64,
    pub schema: String,
    pub table: String,
    pub name: String,
    pub command: String,
    pub roles: Vec<String>,
    pub using: Option<String>,
    pub with_check: Option<String>,
}

impl MetaClient {
    pub fn new(base_url: String, client: reqwest::Client, token: Option<String>) -> Self {
        Self {
            base_url,
            client,
            token,
        }
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

    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        tenant_id: &str,
    ) -> Result<T, PicusError> {
        let url = format!("{}/meta{}", self.base_url, path);
        let mut req = self.client.get(&url);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        req = req.header("x-project-ref", tenant_id);

        let resp = req.send().await.map_err(|e| self.map_err(e))?;
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

    async fn post<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        path: &str,
        body: &B,
        tenant_id: &str,
    ) -> Result<T, PicusError> {
        let url = format!("{}/meta{}", self.base_url, path);
        let mut req = self.client.post(&url).json(body);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        req = req.header("x-project-ref", tenant_id);

        let resp = req.send().await.map_err(|e| self.map_err(e))?;
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

    async fn patch<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        path: &str,
        body: &B,
        tenant_id: &str,
    ) -> Result<T, PicusError> {
        let url = format!("{}/meta{}", self.base_url, path);
        let mut req = self.client.patch(&url).json(body);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        req = req.header("x-project-ref", tenant_id);

        let resp = req.send().await.map_err(|e| self.map_err(e))?;
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

    async fn delete<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        tenant_id: &str,
    ) -> Result<T, PicusError> {
        let url = format!("{}/meta{}", self.base_url, path);
        let mut req = self.client.delete(&url);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        req = req.header("x-project-ref", tenant_id);

        let resp = req.send().await.map_err(|e| self.map_err(e))?;
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

    // --- SQL Utilities ---

    /// Execute SQL via the `/sql` endpoint (different from `/meta/query`).
    pub async fn run_sql(
        &self,
        sql: &str,
        params: Option<Vec<Value>>,
        tenant_id: &str,
    ) -> Result<String, PicusError> {
        self.run_sql_in(sql, params, tenant_id, None).await
    }

    /// Same as `run_sql`, against an explicit database — only meaningful for
    /// `tenant_id == "system"`, where it picks which database the statement runs on.
    pub async fn run_sql_in(
        &self,
        sql: &str,
        params: Option<Vec<Value>>,
        tenant_id: &str,
        database: Option<&str>,
    ) -> Result<String, PicusError> {
        let url = format!("{}/sql", self.base_url);
        let mut req = self.client.post(&url).json(&SqlRequest {
            sql: sql.to_string(),
            params,
            database: database.map(str::to_string),
        });
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        req = req.header("x-project-ref", tenant_id);

        let resp = req.send().await.map_err(|e| self.map_err(e))?;
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
        resp.text().await.map_err(|e| self.map_err(e))
    }

    pub async fn query(
        &self,
        sql: &str,
        params: Option<Vec<Value>>,
        tenant_id: &str,
    ) -> Result<Value, PicusError> {
        self.post(
            "/query",
            &SqlRequest {
                sql: sql.to_string(),
                params,
                database: None,
            },
            tenant_id,
        )
        .await
    }

    pub async fn format(&self, sql: &str) -> Result<Value, PicusError> {
        self.post(
            "/format",
            &SqlRequest {
                sql: sql.to_string(),
                params: None,
                database: None,
            },
            "tenant_001",
        )
        .await
    }

    pub async fn parse(&self, sql: &str) -> Result<Value, PicusError> {
        self.post(
            "/parse",
            &SqlRequest {
                sql: sql.to_string(),
                params: None,
                database: None,
            },
            "tenant_001",
        )
        .await
    }

    pub async fn explain(
        &self,
        sql: &str,
        params: Option<Vec<Value>>,
        tenant_id: &str,
    ) -> Result<Value, PicusError> {
        self.explain_in(sql, params, tenant_id, None).await
    }

    pub async fn explain_in(
        &self,
        sql: &str,
        params: Option<Vec<Value>>,
        tenant_id: &str,
        database: Option<&str>,
    ) -> Result<Value, PicusError> {
        self.post(
            "/explain",
            &SqlRequest {
                sql: sql.to_string(),
                params,
                database: database.map(str::to_string),
            },
            tenant_id,
        )
        .await
    }

    // --- Schemas ---

    pub async fn list_schemas(&self, tenant_id: &str) -> Result<Vec<Schema>, PicusError> {
        self.get("/schemas", tenant_id).await
    }

    pub async fn create_schema(
        &self,
        req: &CreateSchemaRequest,
        tenant_id: &str,
    ) -> Result<Value, PicusError> {
        self.post("/schemas", req, tenant_id).await
    }

    pub async fn alter_schema(
        &self,
        name: &str,
        req: &AlterSchemaRequest,
        tenant_id: &str,
    ) -> Result<Value, PicusError> {
        self.patch(&format!("/schemas/{}", name), req, tenant_id)
            .await
    }

    pub async fn drop_schema(
        &self,
        name: &str,
        cascade: bool,
        tenant_id: &str,
    ) -> Result<Value, PicusError> {
        self.delete(&format!("/schemas/{}?cascade={}", name, cascade), tenant_id)
            .await
    }

    // --- Tables ---

    pub async fn list_tables(
        &self,
        schema: &str,
        tenant_id: &str,
    ) -> Result<Vec<Table>, PicusError> {
        self.get(&format!("/tables?schema={}", schema), tenant_id)
            .await
    }

    pub async fn create_table(
        &self,
        req: &CreateTableRequest,
        tenant_id: &str,
    ) -> Result<Value, PicusError> {
        self.post("/tables", req, tenant_id).await
    }

    pub async fn alter_table(
        &self,
        schema: &str,
        table: &str,
        req: &AlterTableRequest,
        tenant_id: &str,
    ) -> Result<Value, PicusError> {
        self.patch(&format!("/tables/{}/{}", schema, table), req, tenant_id)
            .await
    }

    pub async fn drop_table(
        &self,
        schema: &str,
        table: &str,
        cascade: bool,
        tenant_id: &str,
    ) -> Result<Value, PicusError> {
        self.delete(
            &format!("/tables/{}/{}?cascade={}", schema, table, cascade),
            tenant_id,
        )
        .await
    }

    pub async fn get_table_definition(
        &self,
        schema: &str,
        table: &str,
        tenant_id: &str,
    ) -> Result<String, PicusError> {
        self.get(
            &format!("/tables/{}/{}/definition", schema, table),
            tenant_id,
        )
        .await
    }

    // --- Columns ---

    pub async fn list_columns(
        &self,
        schema: &str,
        table: &str,
        tenant_id: &str,
    ) -> Result<Vec<Column>, PicusError> {
        self.get(
            &format!("/columns?schema={}&table={}", schema, table),
            tenant_id,
        )
        .await
    }

    pub async fn add_column(
        &self,
        schema: &str,
        table: &str,
        req: &CreateColumnRequest,
        tenant_id: &str,
    ) -> Result<Value, PicusError> {
        self.post(&format!("/columns/{}/{}", schema, table), req, tenant_id)
            .await
    }

    pub async fn drop_column(
        &self,
        schema: &str,
        table: &str,
        column: &str,
        cascade: bool,
        tenant_id: &str,
    ) -> Result<Value, PicusError> {
        self.delete(
            &format!(
                "/columns/{}/{}/{}?cascade={}",
                schema, table, column, cascade
            ),
            tenant_id,
        )
        .await
    }

    // --- Advanced Objects ---

    pub async fn list_functions(
        &self,
        schema: &str,
        tenant_id: &str,
    ) -> Result<Vec<Function>, PicusError> {
        self.get(&format!("/functions?schema={}", schema), tenant_id)
            .await
    }

    pub async fn list_triggers(
        &self,
        schema: &str,
        tenant_id: &str,
    ) -> Result<Vec<Trigger>, PicusError> {
        self.get(&format!("/triggers?schema={}", schema), tenant_id)
            .await
    }

    pub async fn list_types(
        &self,
        schema: &str,
        tenant_id: &str,
    ) -> Result<Vec<DatabaseType>, PicusError> {
        self.get(&format!("/types?schema={}", schema), tenant_id)
            .await
    }

pub async fn list_extensions(&self, tenant_id: &str) -> Result<Vec<Extension>, PicusError> {
        self.get("/extensions", tenant_id).await
    }

    pub async fn list_publications(&self, tenant_id: &str) -> Result<Vec<Publication>, PicusError> {
        self.get("/publications", tenant_id).await
    }

    pub async fn list_indexes(
        &self,
        schema: &str,
        tenant_id: &str,
    ) -> Result<Vec<Index>, PicusError> {
        self.get(&format!("/indexes?schema={}", schema), tenant_id)
            .await
    }

    // --- System Metrics ---

    pub async fn get_system_load(&self, tenant_id: &str) -> Result<Vec<SystemLoad>, PicusError> {
        self.get("/system/load", tenant_id).await
    }

    // --- Generators ---

    pub async fn get_version(&self, tenant_id: &str) -> Result<Value, PicusError> {
        self.get("/config/version", tenant_id).await
    }

    pub async fn generate_typescript(
        &self,
        schema: &str,
        tenant_id: &str,
    ) -> Result<String, PicusError> {
        let url = format!(
            "{}/meta/generators/typescript?schema={}",
            self.base_url, schema
        );
        let mut req = self.client.get(&url);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        req = req.header("x-project-ref", tenant_id);

        let resp = req.send().await.map_err(|e| self.map_err(e))?;
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
        resp.text().await.map_err(|e| self.map_err(e))
    }
}
