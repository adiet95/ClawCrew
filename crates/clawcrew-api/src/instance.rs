//! Device identity and instance registration for multi-instance gateways.

use serde::{Deserialize, Serialize};

/// Defines the cryptographic and logical identity of a ClawCrew instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceIdentity {
    /// The unique public identifier of this gateway instance.
    pub instance_id: String,
    /// Public key or signed certificate proving device identity.
    pub public_key_pem: Option<String>,
}

/// Registration details when peering multiple instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceRegistration {
    pub identity: DeviceIdentity,
    /// The base URL or address of the remote gateway.
    pub remote_address: String,
    /// A human-readable label for this instance (e.g., "prod-worker-1").
    pub label: String,
    /// Timestamp of last successful heartbeat.
    pub last_heartbeat: String,
    /// Whether the instance is currently deemed active and reachable.
    pub is_online: bool,
}
