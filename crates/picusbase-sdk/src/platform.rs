use picus_errors::{ErrorCategory, ErrorLevel, PicusError};
use serde::{Deserialize, Serialize};

// ── Response Types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Organization {
    pub id: String,
    pub slug: String,
    pub display_name: String,
    pub role: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct OrgMember {
    pub user_id: String,
    pub role: String,
    pub joined_at: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct OrgUserPool {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub organization_id: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Project {
    pub id: String,
    pub project_ref: String,
    pub name: String,
    pub organization_id: String,
    pub status: String,
    pub created_at: Option<String>,
    pub user_pool_id: Option<String>,
    #[serde(default)]
    pub placement: Option<ProjectPlacement>,
    /// `shared` (default) | `dedicated` — see PLAN-tenant-storage-modes.md.
    #[serde(default = "default_storage_mode")]
    pub storage_mode: String,
    /// `active` | `provisioning` | `failed`
    #[serde(default = "default_storage_status")]
    pub storage_status: String,
}

fn default_storage_mode() -> String {
    "shared".to_string()
}
fn default_storage_status() -> String {
    "active".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProjectPlacement {
    pub mode: String,
    pub requested_shard_id: Option<i32>,
    pub desired_shard_id: Option<i32>,
    pub effective_shard_id: Option<i32>,
    pub route_status: Option<String>,
    pub pgdog_config_version: Option<String>,
    pub lifecycle_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProjectPlacementRequest {
    pub mode: Option<String>,
    pub shard_id: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProjectPlacementShardOption {
    pub shard_id: i32,
    pub label: String,
    pub is_default: bool,
    pub is_available: bool,
    pub active_project_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProjectPlacementOptions {
    pub modes: Vec<String>,
    pub default_shard_id: Option<i32>,
    pub shards: Vec<ProjectPlacementShardOption>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProjectKeys {
    pub anon_key: String,
    pub service_key: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProjectAuthConfig {
    pub user_pool_id: String,
    pub client_id: String,
    pub client_secret: Option<String>,
}

// ── Request Types ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateOrganizationRequest {
    pub display_name: String,
    pub slug: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserPoolRequest {
    pub name: String,
    pub description: Option<String>,
    pub organization_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateOrganizationRequest {
    pub display_name: Option<String>,
    pub slug: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InviteMemberRequest {
    pub email: String,
    pub role: String,
    pub project_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub organization_id: String,
    pub region_id: String,
    pub plan: Option<String>,
    pub placement: Option<ProjectPlacementRequest>,
    pub shard_id: Option<i32>,
    /// `shared` (default), `dedicated`, or `dedicated_sharded`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_mode: Option<String>,
    /// Shard count for `dedicated_sharded` (2 or 4; default 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shard_count: Option<u32>,
    /// Caller-chosen `project_ref` instead of the server's auto-generated
    /// `p_<8 hex>`. Must be unique and a safe SQL/keyspace identifier (see
    /// console-api's `create_project`); omit to keep the old auto-generated
    /// behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_ref: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShardUsage {
    pub shard: String,
    pub size_bytes: i64,
    pub rows: i64,
    pub tablet_type: String,
    pub tablet_state: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageSummary {
    pub shared: i64,
    pub dedicated: i64,
    pub dedicated_sharded: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowStatus {
    pub name: String,
    pub streams_total: u32,
    pub streams_running: u32,
    pub streams_error: u32,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageUsage {
    pub storage_mode: String,
    pub storage_status: String,
    #[serde(default)]
    pub keyspace_name: Option<String>,
    pub shard_count: u32,
    pub total_size_bytes: i64,
    pub total_rows: i64,
    pub table_count: i64,
    #[serde(default)]
    pub shards: Vec<ShardUsage>,
    #[serde(default)]
    pub workflow: Option<WorkflowStatus>,
    /// Last reshard failure message, when `storage_status == "reshard_failed"`.
    #[serde(default)]
    pub storage_error: Option<String>,
    /// Requested shard count while a reshard is in flight.
    #[serde(default)]
    pub reshard_target: Option<u32>,
}

// ── Client ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PlatformClient {
    base_url: String,
    client: reqwest::Client,
}

impl PlatformClient {
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

    fn api_err(&self, status: u16, text: String) -> PicusError {
        PicusError::new(
            "SDK",
            ErrorCategory::API,
            ErrorLevel::Error,
            status,
            format!("API Error {}: {}", status, text),
        )
    }

    async fn get_with_bearer<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        access_token: &str,
    ) -> Result<T, PicusError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| self.map_err(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(self.api_err(status.as_u16(), text));
        }
        resp.json::<T>().await.map_err(|e| self.map_err(e))
    }

    async fn post_with_bearer<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        path: &str,
        access_token: &str,
        body: &B,
    ) -> Result<T, PicusError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(access_token)
            .json(body)
            .send()
            .await
            .map_err(|e| self.map_err(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(self.api_err(status.as_u16(), text));
        }
        resp.json::<T>().await.map_err(|e| self.map_err(e))
    }

    async fn post_with_bearer_unit<B: Serialize>(
        &self,
        path: &str,
        access_token: &str,
        body: &B,
    ) -> Result<(), PicusError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(access_token)
            .json(body)
            .send()
            .await
            .map_err(|e| self.map_err(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(self.api_err(status.as_u16(), text));
        }
        Ok(())
    }

    async fn patch_with_bearer<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        path: &str,
        access_token: &str,
        body: &B,
    ) -> Result<T, PicusError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .patch(&url)
            .bearer_auth(access_token)
            .json(body)
            .send()
            .await
            .map_err(|e| self.map_err(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(self.api_err(status.as_u16(), text));
        }
        resp.json::<T>().await.map_err(|e| self.map_err(e))
    }

    async fn delete_with_bearer_unit(
        &self,
        path: &str,
        access_token: &str,
    ) -> Result<(), PicusError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .delete(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| self.map_err(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(self.api_err(status.as_u16(), text));
        }
        Ok(())
    }

    // ── Organization Methods ─────────────────────────────────────────────────

    pub async fn list_organizations_with_bearer(
        &self,
        access_token: &str,
    ) -> Result<Vec<Organization>, PicusError> {
        self.get_with_bearer("/platform/organizations", access_token)
            .await
    }

    pub async fn create_organization_with_bearer(
        &self,
        access_token: &str,
        req: &CreateOrganizationRequest,
    ) -> Result<Organization, PicusError> {
        self.post_with_bearer("/platform/organizations", access_token, req)
            .await
    }

    pub async fn update_organization_with_bearer(
        &self,
        access_token: &str,
        organization_id: &str,
        req: &UpdateOrganizationRequest,
    ) -> Result<Organization, PicusError> {
        self.patch_with_bearer(
            &format!("/platform/organizations/{}", organization_id),
            access_token,
            req,
        )
        .await
    }

    pub async fn delete_organization_with_bearer(
        &self,
        access_token: &str,
        organization_id: &str,
    ) -> Result<(), PicusError> {
        self.delete_with_bearer_unit(
            &format!("/platform/organizations/{}", organization_id),
            access_token,
        )
        .await
    }

    pub async fn list_org_members_with_bearer(
        &self,
        access_token: &str,
        organization_id: &str,
    ) -> Result<Vec<OrgMember>, PicusError> {
        self.get_with_bearer(
            &format!("/platform/organizations/{}/members", organization_id),
            access_token,
        )
        .await
    }

    pub async fn list_org_user_pools_with_bearer(
        &self,
        access_token: &str,
        organization_id: &str,
    ) -> Result<Vec<OrgUserPool>, PicusError> {
        self.get_with_bearer(
            &format!("/platform/organizations/{}/user-pools", organization_id),
            access_token,
        )
        .await
    }

    pub async fn create_org_user_pool_with_bearer(
        &self,
        access_token: &str,
        organization_id: &str,
        req: &CreateUserPoolRequest,
    ) -> Result<OrgUserPool, PicusError> {
        self.post_with_bearer(
            &format!("/platform/organizations/{}/user-pools", organization_id),
            access_token,
            req,
        )
        .await
    }

    pub async fn invite_member_with_bearer(
        &self,
        access_token: &str,
        organization_id: &str,
        req: &InviteMemberRequest,
    ) -> Result<(), PicusError> {
        self.post_with_bearer_unit(
            &format!("/platform/organizations/{}/invite", organization_id),
            access_token,
            req,
        )
        .await
    }

    pub async fn remove_member_with_bearer(
        &self,
        access_token: &str,
        organization_id: &str,
        member_user_id: &str,
    ) -> Result<(), PicusError> {
        self.delete_with_bearer_unit(
            &format!(
                "/platform/organizations/{}/members/{}",
                organization_id, member_user_id
            ),
            access_token,
        )
        .await
    }

    // ── Project Methods ──────────────────────────────────────────────────────

    pub async fn create_project_with_bearer(
        &self,
        access_token: &str,
        req: &CreateProjectRequest,
    ) -> Result<Project, PicusError> {
        self.post_with_bearer("/platform/projects", access_token, req)
            .await
    }

    pub async fn list_project_placement_options_with_bearer(
        &self,
        access_token: &str,
        organization_id: Option<&str>,
    ) -> Result<ProjectPlacementOptions, PicusError> {
        let path = match organization_id {
            Some(org_id) => format!(
                "/platform/project-placement/options?organization_id={}",
                org_id
            ),
            None => "/platform/project-placement/options".to_string(),
        };
        self.get_with_bearer(&path, access_token).await
    }

    pub async fn get_project_with_bearer(
        &self,
        access_token: &str,
        project_ref: &str,
    ) -> Result<Project, PicusError> {
        self.get_with_bearer(&format!("/platform/projects/{}", project_ref), access_token)
            .await
    }

    pub async fn update_project_with_bearer(
        &self,
        access_token: &str,
        project_ref: &str,
        req: &UpdateProjectRequest,
    ) -> Result<Project, PicusError> {
        self.patch_with_bearer(
            &format!("/platform/projects/{}", project_ref),
            access_token,
            req,
        )
        .await
    }

    /// Promote a `shared` project to a `dedicated` Vitess keyspace
    /// (PLAN-tenant-storage-modes.md). Returns immediately with
    /// `storage_status = "provisioning"`; poll `get_project` for the result.
    pub async fn promote_project_storage_with_bearer(
        &self,
        access_token: &str,
        project_ref: &str,
    ) -> Result<Project, PicusError> {
        self.post_with_bearer(
            &format!("/platform/projects/{}/storage/promote", project_ref),
            access_token,
            &(),
        )
        .await
    }

    /// Giai đoạn 3 — start sharding a `dedicated` project (`shard_count`
    /// 2 or 4). Background; `storage_status` → `sharding`. Then `cutover`,
    /// then `complete` (or `abort`).
    pub async fn shard_project_storage_with_bearer(
        &self,
        access_token: &str,
        project_ref: &str,
        shard_count: u32,
    ) -> Result<Project, PicusError> {
        self.post_with_bearer(
            &format!("/platform/projects/{}/storage/shard", project_ref),
            access_token,
            &serde_json::json!({ "shard_count": shard_count }),
        )
        .await
    }

    pub async fn shard_abort_storage_with_bearer(
        &self,
        access_token: &str,
        project_ref: &str,
    ) -> Result<Project, PicusError> {
        self.post_with_bearer(
            &format!("/platform/projects/{}/storage/shard/abort", project_ref),
            access_token,
            &(),
        )
        .await
    }

    /// Live reshard a `dedicated_sharded` project to `shard_count` shards
    /// (1/2/4/8). Background; `storage_status` → `resharding`, then
    /// `reshard_cutover`, then `complete` (or `abort`).
    pub async fn reshard_storage_with_bearer(
        &self,
        access_token: &str,
        project_ref: &str,
        shard_count: u32,
    ) -> Result<Project, PicusError> {
        self.post_with_bearer(
            &format!("/platform/projects/{}/storage/reshard", project_ref),
            access_token,
            &serde_json::json!({ "shard_count": shard_count }),
        )
        .await
    }

    pub async fn reshard_abort_storage_with_bearer(
        &self,
        access_token: &str,
        project_ref: &str,
    ) -> Result<Project, PicusError> {
        self.post_with_bearer(
            &format!("/platform/projects/{}/storage/reshard/abort", project_ref),
            access_token,
            &(),
        )
        .await
    }

    pub async fn project_storage_usage_with_bearer(
        &self,
        access_token: &str,
        project_ref: &str,
    ) -> Result<StorageUsage, PicusError> {
        self.get_with_bearer(
            &format!("/platform/projects/{}/storage/usage", project_ref),
            access_token,
        )
        .await
    }

    pub async fn platform_storage_summary_with_bearer(
        &self,
        access_token: &str,
    ) -> Result<StorageSummary, PicusError> {
        self.get_with_bearer("/platform/storage/summary", access_token)
            .await
    }

    pub async fn delete_project_with_bearer(
        &self,
        access_token: &str,
        project_ref: &str,
    ) -> Result<(), PicusError> {
        self.delete_with_bearer_unit(&format!("/platform/projects/{}", project_ref), access_token)
            .await
    }

    pub async fn list_projects_with_bearer(
        &self,
        access_token: &str,
        organization_id: Option<&str>,
    ) -> Result<Vec<Project>, PicusError> {
        let path = match organization_id {
            Some(org_id) => format!("/platform/projects?organization_id={}", org_id),
            None => "/platform/projects".to_string(),
        };
        self.get_with_bearer(&path, access_token).await
    }

    pub async fn get_project_keys_with_bearer(
        &self,
        access_token: &str,
        project_ref: &str,
    ) -> Result<ProjectKeys, PicusError> {
        self.get_with_bearer(
            &format!("/platform/v1/projects/{}/api-keys", project_ref),
            access_token,
        )
        .await
    }

    pub async fn rotate_project_keys_with_bearer(
        &self,
        access_token: &str,
        project_ref: &str,
    ) -> Result<ProjectKeys, PicusError> {
        let url = format!(
            "{}/platform/v1/projects/{}/api-keys/rotate",
            self.base_url, project_ref
        );
        let resp = self
            .client
            .post(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| self.map_err(e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(self.api_err(status.as_u16(), text));
        }
        resp.json().await.map_err(|e| self.map_err(e))
    }

    pub async fn get_project_auth_config_with_bearer(
        &self,
        access_token: &str,
        project_ref: &str,
    ) -> Result<ProjectAuthConfig, PicusError> {
        self.get_with_bearer(
            &format!("/platform/projects/{}/auth-config", project_ref),
            access_token,
        )
        .await
    }
}
