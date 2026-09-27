//! Local plugin scaffolding and fixture generation (P3.3).
//!
//! Produces a minimal, parseable `manifest.toml` for a new plugin so local
//! development has a valid starting point without hand-copying a manifest.

use crate::{PluginCapability, PluginManifest};

/// Capability name as it appears in a manifest (`snake_case`).
fn capability_name(capability: PluginCapability) -> &'static str {
    match capability {
        PluginCapability::Tool => "tool",
        PluginCapability::Channel => "channel",
        PluginCapability::Memory => "memory",
        PluginCapability::Observer => "observer",
        PluginCapability::Skill => "skill",
    }
}

/// Render a minimal `manifest.toml` scaffold for a new plugin.
#[must_use]
pub fn scaffold_plugin_manifest_toml(
    name: &str,
    version: &str,
    capability: PluginCapability,
) -> String {
    format!(
        "# ClawCrew plugin manifest scaffold — edit before publishing.\n\
         name = \"{name}\"\n\
         version = \"{version}\"\n\
         description = \"A new ClawCrew plugin\"\n\
         author = \"\"\n\
         capabilities = [\"{capability}\"]\n\
         permissions = []\n",
        capability = capability_name(capability),
    )
}

/// Build and parse a plugin manifest fixture from the scaffold.
pub fn scaffold_plugin_manifest(
    name: &str,
    version: &str,
    capability: PluginCapability,
) -> Result<PluginManifest, toml::de::Error> {
    toml::from_str(&scaffold_plugin_manifest_toml(name, version, capability))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_manifest_parses_for_every_capability() {
        for capability in [
            PluginCapability::Tool,
            PluginCapability::Channel,
            PluginCapability::Memory,
            PluginCapability::Observer,
            PluginCapability::Skill,
        ] {
            let manifest = scaffold_plugin_manifest("demo", "0.1.0", capability)
                .expect("scaffold must parse");
            assert_eq!(manifest.name, "demo");
            assert_eq!(manifest.version, "0.1.0");
            assert_eq!(manifest.capabilities, vec![capability]);
            // A fresh scaffold is unsigned by default.
            assert!(manifest.signature.is_none());
            assert!(manifest.publisher_key.is_none());
        }
    }

    #[test]
    fn scaffold_toml_mentions_the_capability() {
        let toml = scaffold_plugin_manifest_toml("demo", "0.1.0", PluginCapability::Tool);
        assert!(toml.contains("capabilities = [\"tool\"]"));
        assert!(toml.contains("name = \"demo\""));
    }
}