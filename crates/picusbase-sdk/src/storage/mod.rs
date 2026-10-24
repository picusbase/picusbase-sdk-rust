use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BucketInfo {
    pub name: String,
    pub created_at: Option<String>,
    pub max_size: Option<u64>,
}

#[cfg(not(target_arch = "wasm32"))]
pub mod server;

pub mod client;

#[cfg(not(target_arch = "wasm32"))]
pub use server::*;

pub use client::*;
