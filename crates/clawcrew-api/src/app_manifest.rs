//! Versioned App and plugin manifest contract.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Deny-by-default capability grants requested by an extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityGrant {
    /// Allow the extension to access the network.
    NetworkAccess,
    /// Allow the extension to read the file system.
    FsRead,
    /// Allow the extension to write to the file system.
    FsWrite,
    /// Allow the extension to spawn subprocesses.
    Subprocess,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppManifest {
    /// The unique identifier of the App.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Version string of the extension.
    pub version: String,
    /// Minimum ClawCrew runtime compatibility version required.
    pub min_runtime_version: String,
    /// Other Apps that must be enabled before this App can activate.
    #[serde(default)]
    pub dependencies: Vec<AppDependency>,
    /// A list of permissions required by this App.
    #[serde(default)]
    pub permissions: Vec<CapabilityGrant>,
    /// The tools exposed by this App to the agent.
    #[serde(default)]
    pub tools: Vec<AppToolDefinition>,
    /// Configuration schema owned by the App.
    #[serde(default = "default_config")]
    pub config: serde_json::Value,
    /// External UI routes or custom pages provided by the App.
    #[serde(default)]
    pub ui_routes: HashMap<String, String>,
    /// Lifecycle hook names routed through the core runtime.
    #[serde(default)]
    pub lifecycle_hooks: Vec<String>,
    /// Optional MCP server configuration if this App represents an MCP server.
    #[serde(default)]
    pub mcp_server: Option<McpServerDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerDefinition {
    /// Command to start the MCP server.
    pub command: String,
    /// Arguments to pass to the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_config() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppDependency {
    pub id: String,
    pub min_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppToolDefinition {
    /// The name of the tool exposed to the agent.
    pub name: String,
    /// The description of the tool.
    pub description: String,
    /// Entrypoint or execution hook inside the plugin.
    pub entrypoint: String,
}
