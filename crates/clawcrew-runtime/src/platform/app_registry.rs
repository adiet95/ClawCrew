//! App and plugin registry. Loads `AppManifest` definitions, registers tools,
//! and (when opened over a data dir) persists install/enable state durably.
//!
//! Durability is scoped to the registry's own file
//! (`<data_dir>/apps/registry.json`). Install/enable/update/rollback/remove
//! never touch the control-plane, session, or memory stores, so App lifecycle
//! cannot corrupt task/session state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use clawcrew_api::app_manifest::AppManifest;

/// Version of the persisted registry payload. A higher on-disk version is
/// refused rather than partially read (downgrade is unsafe).
pub const APP_REGISTRY_SCHEMA_VERSION: u32 = 1;

const REGISTRY_REL_PATH: &str = "apps/registry.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppState {
    Installed,
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppEntry {
    manifest: AppManifest,
    state: AppState,
    #[serde(default)]
    previous_manifest: Option<AppManifest>,
}

/// The on-disk shape of the registry.
#[derive(Debug, Serialize, Deserialize)]
struct PersistedRegistry {
    /// Missing/`0` in legacy payloads; see [`AppRegistry::open`].
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    apps: Vec<PersistedApp>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedApp {
    manifest: AppManifest,
    state: AppState,
    #[serde(default)]
    previous_manifest: Option<AppManifest>,
}

pub struct AppRegistry {
    apps: HashMap<String, AppEntry>,
    /// When set, every mutation is persisted to this directory.
    store: Option<PathBuf>,
}

impl Default for AppRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AppRegistry {
    /// In-memory registry (no persistence). Used by tests and one-shot callers.
    pub fn new() -> Self {
        Self {
            apps: HashMap::new(),
            store: None,
        }
    }

    /// Open the durable registry under `<data_dir>/apps/registry.json`.
    ///
    /// A legacy payload without `schema_version` is migrated to the current
    /// version and rewritten. A payload written by a newer runtime is refused
    /// (downgrade would silently drop install state). The load only ever reads
    /// this file, so it cannot corrupt task/session state.
    pub fn open(data_dir: &Path) -> Result<Self, anyhow::Error> {
        let path = data_dir.join(REGISTRY_REL_PATH);
        let mut registry = Self {
            apps: HashMap::new(),
            store: Some(data_dir.to_path_buf()),
        };
        if !path.exists() {
            return Ok(registry);
        }
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("read app registry {}: {e}", path.display()))?;
        let persisted: PersistedRegistry = serde_json::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("parse app registry {}: {e}", path.display()))?;
        anyhow::ensure!(
            persisted.schema_version <= APP_REGISTRY_SCHEMA_VERSION,
            "app registry schema {} is newer than supported {}",
            persisted.schema_version,
            APP_REGISTRY_SCHEMA_VERSION
        );
        let migrated = persisted.schema_version < APP_REGISTRY_SCHEMA_VERSION;
        for app in persisted.apps {
            registry.apps.insert(
                app.manifest.id.clone(),
                AppEntry {
                    manifest: app.manifest,
                    state: app.state,
                    previous_manifest: app.previous_manifest,
                },
            );
        }
        if migrated {
            // Persist the upgraded payload so the migration is one-time.
            registry.persist()?;
        }
        Ok(registry)
    }

    fn persist(&self) -> Result<(), anyhow::Error> {
        let Some(data_dir) = self.store.as_ref() else {
            return Ok(());
        };
        let dir = data_dir.join("apps");
        std::fs::create_dir_all(&dir)
            .map_err(|e| anyhow::anyhow!("create app registry dir {}: {e}", dir.display()))?;
        let payload = PersistedRegistry {
            schema_version: APP_REGISTRY_SCHEMA_VERSION,
            apps: self
                .apps
                .values()
                .map(|entry| PersistedApp {
                    manifest: entry.manifest.clone(),
                    state: entry.state,
                    previous_manifest: entry.previous_manifest.clone(),
                })
                .collect(),
        };
        let serialized = serde_json::to_string_pretty(&payload)?;
        // Atomic replace: a crashed write must not leave a truncated registry.
        let tmp = dir.join("registry.json.tmp");
        let final_path = dir.join("registry.json");
        std::fs::write(&tmp, serialized)
            .map_err(|e| anyhow::anyhow!("write app registry {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &final_path)
            .map_err(|e| anyhow::anyhow!("commit app registry {}: {e}", final_path.display()))?;
        Ok(())
    }

    /// Load an AppManifest and validate its requested permissions against core policies.
    pub fn register_app(&mut self, manifest: AppManifest) -> Result<(), anyhow::Error> {
        validate_manifest(&manifest)?;
        self.apps.insert(
            manifest.id.clone(),
            AppEntry {
                manifest,
                state: AppState::Installed,
                previous_manifest: None,
            },
        );
        self.persist()
    }

    pub fn get_app(&self, id: &str) -> Option<&AppManifest> {
        self.apps.get(id).map(|entry| &entry.manifest)
    }

    pub fn list_apps(&self) -> Vec<&AppManifest> {
        self.apps.values().map(|entry| &entry.manifest).collect()
    }

    pub fn state(&self, id: &str) -> Option<AppState> {
        self.apps.get(id).map(|entry| entry.state)
    }

    pub fn enable_app(&mut self, id: &str, runtime_version: &str) -> Result<(), anyhow::Error> {
        let entry = self
            .apps
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("unknown App {id}"))?;
        validate_runtime_version(&entry.manifest.min_runtime_version, runtime_version)?;
        for dependency in &entry.manifest.dependencies {
            let dependency_entry = self
                .apps
                .get(&dependency.id)
                .ok_or_else(|| anyhow::anyhow!("missing App dependency {}", dependency.id))?;
            anyhow::ensure!(
                dependency_entry.state == AppState::Enabled,
                "App dependency {} is not enabled",
                dependency.id
            );
            validate_runtime_version(&dependency.min_version, &dependency_entry.manifest.version)?;
        }
        self.apps
            .get_mut(id)
            .expect("entry was checked above")
            .state = AppState::Enabled;
        self.persist()
    }

    pub fn disable_app(&mut self, id: &str) -> Result<(), anyhow::Error> {
        let entry = self
            .apps
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("unknown App {id}"))?;
        entry.state = AppState::Disabled;
        self.persist()
    }

    pub fn update_app(&mut self, manifest: AppManifest) -> Result<(), anyhow::Error> {
        validate_manifest(&manifest)?;
        let entry = self
            .apps
            .get_mut(&manifest.id)
            .ok_or_else(|| anyhow::anyhow!("unknown App {}", manifest.id))?;
        entry.previous_manifest = Some(std::mem::replace(&mut entry.manifest, manifest));
        self.persist()
    }

    pub fn rollback_app(&mut self, id: &str) -> Result<(), anyhow::Error> {
        let entry = self
            .apps
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("unknown App {id}"))?;
        let previous = entry
            .previous_manifest
            .take()
            .ok_or_else(|| anyhow::anyhow!("no rollback version for App {id}"))?;
        entry.manifest = previous;
        self.persist()
    }

    /// Remove an App and its install/rollback metadata. Removal is scoped to the
    /// registry file only — it never touches task, session, or memory state.
    pub fn remove_app(&mut self, id: &str) -> Result<(), anyhow::Error> {
        let entry = self
            .apps
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("unknown App {id}"))?;
        anyhow::ensure!(
            entry.state != AppState::Enabled,
            "disable App {id} before removal"
        );
        self.apps.remove(id);
        self.persist()
    }

    /// Keep App grants deny-by-default and route all tools through core approval/audit.
    pub fn is_tool_approved(&self, _app_id: &str, _tool: &str) -> bool {
        false
    }

    /// Authorize an App tool execution through core governance. An App can never
    /// widen policy: the App must be installed and enabled, the tool must be
    /// declared by its manifest, and the caller-supplied core approval decision
    /// (from `ApprovalManager`/audit) must allow it. The verdict is a projection
    /// of core governance, never App-managed.
    pub fn authorize_tool(
        &self,
        app_id: &str,
        tool: &str,
        core_approved: bool,
    ) -> AppToolVerdict {
        let Some(entry) = self.apps.get(app_id) else {
            return AppToolVerdict::AppNotEnabled;
        };
        if entry.state != AppState::Enabled {
            return AppToolVerdict::AppNotEnabled;
        }
        if !entry.manifest.tools.iter().any(|declared| declared.name == tool) {
            return AppToolVerdict::UndeclaredTool;
        }
        if !core_approved {
            return AppToolVerdict::ApprovalDenied;
        }
        AppToolVerdict::Allowed
    }
}

/// The outcome of authorizing an App tool execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppToolVerdict {
    Allowed,
    /// The App is unknown or not enabled.
    AppNotEnabled,
    /// The App does not declare this tool.
    UndeclaredTool,
    /// Core governance denied the execution.
    ApprovalDenied,
}

fn validate_manifest(manifest: &AppManifest) -> Result<(), anyhow::Error> {
    anyhow::ensure!(!manifest.id.is_empty(), "App id cannot be empty");
    anyhow::ensure!(!manifest.name.is_empty(), "App name cannot be empty");
    anyhow::ensure!(!manifest.version.is_empty(), "App version cannot be empty");
    anyhow::ensure!(
        !manifest.min_runtime_version.is_empty(),
        "App minimum runtime version cannot be empty"
    );
    let mut tool_names = std::collections::HashSet::new();
    for tool in &manifest.tools {
        anyhow::ensure!(!tool.name.is_empty(), "App tool name cannot be empty");
        anyhow::ensure!(
            tool_names.insert(&tool.name),
            "duplicate App tool {}",
            tool.name
        );
    }
    for hook in &manifest.lifecycle_hooks {
        anyhow::ensure!(!hook.is_empty(), "App lifecycle hook cannot be empty");
    }
    Ok(())
}

fn version_tuple(value: &str) -> Result<(u64, u64, u64), anyhow::Error> {
    let core = value.split(['-', '+']).next().unwrap_or(value);
    let mut parts = core.split('.');
    let major = parts.next().unwrap_or("0").parse()?;
    let minor = parts.next().unwrap_or("0").parse()?;
    let patch = parts.next().unwrap_or("0").parse()?;
    anyhow::ensure!(parts.next().is_none(), "invalid version {value}");
    Ok((major, minor, patch))
}

fn validate_runtime_version(minimum: &str, actual: &str) -> Result<(), anyhow::Error> {
    anyhow::ensure!(
        version_tuple(actual)? >= version_tuple(minimum)?,
        "runtime version {actual} does not satisfy minimum {minimum}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawcrew_api::app_manifest::{AppDependency, AppToolDefinition};

    fn manifest(id: &str, version: &str) -> AppManifest {
        AppManifest {
            id: id.into(),
            name: id.into(),
            version: version.into(),
            min_runtime_version: "1.0.0".into(),
            dependencies: Vec::new(),
            permissions: Vec::new(),
            tools: vec![AppToolDefinition {
                name: format!("{id}-tool"),
                description: "test tool".into(),
                entrypoint: "run".into(),
            }],
            config: serde_json::json!({"enabled": true}),
            ui_routes: HashMap::new(),
            lifecycle_hooks: vec!["on_enable".into()],
        }
    }

    #[test]
    fn lifecycle_requires_compatible_enabled_dependencies() {
        let mut registry = AppRegistry::new();
        registry.register_app(manifest("base", "1.2.0")).unwrap();
        let mut child = manifest("child", "1.0.0");
        child.dependencies = vec![AppDependency {
            id: "base".into(),
            min_version: "1.1.0".into(),
        }];
        registry.register_app(child).unwrap();
        assert!(registry.enable_app("child", "1.0.0").is_err());
        registry.enable_app("base", "1.0.0").unwrap();
        registry.enable_app("child", "1.0.0").unwrap();
        assert_eq!(registry.state("child"), Some(AppState::Enabled));
    }

    #[test]
    fn update_rollback_and_remove_preserve_lifecycle_safety() {
        let mut registry = AppRegistry::new();
        registry.register_app(manifest("app", "1.0.0")).unwrap();
        let mut updated = manifest("app", "2.0.0");
        updated.config = serde_json::json!({"version": 2});
        registry.update_app(updated).unwrap();
        assert_eq!(registry.get_app("app").unwrap().version, "2.0.0");
        registry.rollback_app("app").unwrap();
        assert_eq!(registry.get_app("app").unwrap().version, "1.0.0");
        registry.remove_app("app").unwrap();
        assert!(registry.get_app("app").is_none());
    }

    #[test]
    fn tool_grants_are_deny_by_default_even_when_enabled() {
        let mut registry = AppRegistry::new();
        registry.register_app(manifest("app", "1.0.0")).unwrap();
        registry.enable_app("app", "1.0.0").unwrap();
        // No App can widen policy: every tool stays denied regardless of
        // registration/enable state (P2.2 policy-bypass boundary).
        assert!(!registry.is_tool_approved("app", "app-tool"));
        assert!(!registry.is_tool_approved("app", "shell"));
        assert!(!registry.is_tool_approved("unknown", "anything"));
    }

    #[test]
    fn authorize_tool_requires_enabled_declared_and_core_approval() {
        let mut registry = AppRegistry::new();
        registry.register_app(manifest("app", "1.0.0")).unwrap();

        // Installed but not enabled -> refused.
        assert_eq!(
            registry.authorize_tool("app", "app-tool", true),
            AppToolVerdict::AppNotEnabled
        );

        registry.enable_app("app", "1.0.0").unwrap();
        // Enabled but core governance denied -> refused.
        assert_eq!(
            registry.authorize_tool("app", "app-tool", false),
            AppToolVerdict::ApprovalDenied
        );
        // Undeclared tool -> refused even if core approves.
        assert_eq!(
            registry.authorize_tool("app", "shell", true),
            AppToolVerdict::UndeclaredTool
        );
        // Enabled + declared + core approved -> allowed.
        assert_eq!(
            registry.authorize_tool("app", "app-tool", true),
            AppToolVerdict::Allowed
        );
        // Unknown App -> refused.
        assert_eq!(
            registry.authorize_tool("nope", "app-tool", true),
            AppToolVerdict::AppNotEnabled
        );
    }

    #[test]
    fn activation_rejects_incompatible_runtime_and_dependency_versions() {
        let mut registry = AppRegistry::new();
        registry.register_app(manifest("app", "1.0.0")).unwrap();
        // Runtime below the manifest minimum is refused, leaving it installed.
        assert!(registry.enable_app("app", "0.9.0").is_err());
        assert_eq!(registry.state("app"), Some(AppState::Installed));

        // A dependency pinned above the provider's actual version is refused.
        registry.register_app(manifest("base", "1.0.0")).unwrap();
        registry.enable_app("base", "1.0.0").unwrap();
        let mut child = manifest("child", "1.0.0");
        child.dependencies = vec![AppDependency {
            id: "base".into(),
            min_version: "2.0.0".into(),
        }];
        registry.register_app(child).unwrap();
        assert!(registry.enable_app("child", "1.0.0").is_err());
        assert_eq!(registry.state("child"), Some(AppState::Installed));
    }

    #[test]
    fn invalid_manifest_and_unguarded_removal_are_rejected() {
        let mut registry = AppRegistry::new();
        let mut duplicate_tool = manifest("app", "1.0.0");
        duplicate_tool.tools.push(duplicate_tool.tools[0].clone());
        assert!(registry.register_app(duplicate_tool).is_err());

        let mut empty_id = manifest("app", "1.0.0");
        empty_id.id = String::new();
        assert!(registry.register_app(empty_id).is_err());

        // An enabled App must be disabled before removal.
        registry.register_app(manifest("app", "1.0.0")).unwrap();
        registry.enable_app("app", "1.0.0").unwrap();
        assert!(registry.remove_app("app").is_err());
        registry.disable_app("app").unwrap();
        assert!(registry.remove_app("app").is_ok());
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zc-appreg-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn durable_registry_round_trips_install_and_enable_state() {
        let dir = temp_dir("roundtrip");
        {
            let mut registry = AppRegistry::open(&dir).unwrap();
            registry.register_app(manifest("base", "1.0.0")).unwrap();
            registry.enable_app("base", "1.0.0").unwrap();
            let mut child = manifest("child", "1.0.0");
            child.dependencies = vec![AppDependency {
                id: "base".into(),
                min_version: "1.0.0".into(),
            }];
            registry.register_app(child).unwrap();
        }
        let reopened = AppRegistry::open(&dir).unwrap();
        assert_eq!(reopened.state("base"), Some(AppState::Enabled));
        assert_eq!(reopened.state("child"), Some(AppState::Installed));
        assert_eq!(reopened.list_apps().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_payload_without_schema_version_migrates_and_rewrites() {
        let dir = temp_dir("migrate");
        let apps_dir = dir.join("apps");
        std::fs::create_dir_all(&apps_dir).unwrap();
        // A v0 payload: no `schema_version` key.
        let legacy = serde_json::json!({
            "apps": [{
                "manifest": manifest("legacy", "1.0.0"),
                "state": "enabled"
            }]
        });
        std::fs::write(
            apps_dir.join("registry.json"),
            serde_json::to_string_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let registry = AppRegistry::open(&dir).unwrap();
        assert_eq!(registry.state("legacy"), Some(AppState::Enabled));

        // The migration rewrote the payload at the current version.
        let rewritten: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(apps_dir.join("registry.json")).unwrap())
                .unwrap();
        assert_eq!(
            rewritten["schema_version"].as_u64(),
            Some(u64::from(APP_REGISTRY_SCHEMA_VERSION))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn newer_registry_schema_is_refused_without_partial_load() {
        let dir = temp_dir("newer");
        let apps_dir = dir.join("apps");
        std::fs::create_dir_all(&apps_dir).unwrap();
        std::fs::write(
            apps_dir.join("registry.json"),
            serde_json::to_string(&serde_json::json!({
                "schema_version": APP_REGISTRY_SCHEMA_VERSION + 1,
                "apps": []
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(AppRegistry::open(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removal_cleans_up_only_the_target_app() {
        let dir = temp_dir("cleanup");
        {
            let mut registry = AppRegistry::open(&dir).unwrap();
            registry.register_app(manifest("keep", "1.0.0")).unwrap();
            registry.register_app(manifest("drop", "1.0.0")).unwrap();
            registry.remove_app("drop").unwrap();
        }
        let reopened = AppRegistry::open(&dir).unwrap();
        assert!(reopened.get_app("keep").is_some());
        assert!(reopened.get_app("drop").is_none());
        assert_eq!(reopened.list_apps().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}