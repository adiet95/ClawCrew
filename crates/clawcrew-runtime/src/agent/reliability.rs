//! Provider reliability, fallback routing, and capability matrix.
//!
//! These records are the runtime-side contract for P1.5: they carry the
//! capability, health, cooldown, latency, and reliability signals that route
//! selection consults, and [`select_route`] turns them into an explainable
//! decision that names the served provider when a fallback occurred.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Cooldown is applied after this many consecutive failures.
const COOLDOWN_FAILURE_THRESHOLD: u32 = 3;

/// Default cooldown minutes recorded by the live provider-health registry.
const PROVIDER_COOLDOWN_MINUTES: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealthRecord {
    pub provider_name: String,
    pub total_invocations: u64,
    pub successful_invocations: u64,
    pub consecutive_failures: u32,
    pub in_cooldown_until: Option<String>,
    /// Sum of observed latencies in milliseconds (for the average).
    #[serde(default)]
    pub total_latency_ms: u64,
    /// Most recent observed latency in milliseconds.
    #[serde(default)]
    pub last_latency_ms: Option<u64>,
}

impl ProviderHealthRecord {
    pub fn new(provider_name: String) -> Self {
        Self {
            provider_name,
            total_invocations: 0,
            successful_invocations: 0,
            consecutive_failures: 0,
            in_cooldown_until: None,
            total_latency_ms: 0,
            last_latency_ms: None,
        }
    }

    pub fn record_success(&mut self) {
        self.total_invocations += 1;
        self.successful_invocations += 1;
        self.consecutive_failures = 0;
        self.in_cooldown_until = None;
    }

    /// Record a success and the observed latency in milliseconds.
    pub fn record_success_with_latency(&mut self, latency_ms: u64) {
        self.record_success();
        self.observe_latency(latency_ms);
    }

    pub fn record_failure(&mut self, cooldown_minutes: u32) {
        self.total_invocations += 1;
        self.consecutive_failures += 1;
        if self.consecutive_failures > COOLDOWN_FAILURE_THRESHOLD {
            let now = chrono::Utc::now();
            let cooldown = now + chrono::Duration::minutes(cooldown_minutes as i64);
            self.in_cooldown_until = Some(cooldown.to_rfc3339());
        }
    }

    /// Record a failure and the observed latency in milliseconds.
    pub fn record_failure_with_latency(&mut self, cooldown_minutes: u32, latency_ms: u64) {
        self.record_failure(cooldown_minutes);
        self.observe_latency(latency_ms);
    }

    fn observe_latency(&mut self, latency_ms: u64) {
        self.total_latency_ms = self.total_latency_ms.saturating_add(latency_ms);
        self.last_latency_ms = Some(latency_ms);
    }

    pub fn is_in_cooldown(&self) -> bool {
        if let Some(ref until_str) = self.in_cooldown_until {
            if let Ok(until_time) = chrono::DateTime::parse_from_rfc3339(until_str) {
                return chrono::Utc::now() < until_time.with_timezone(&chrono::Utc);
            }
        }
        false
    }

    /// Observed success ratio in `[0.0, 1.0]`; `1.0` when never invoked (an
    /// unproven provider is not treated as unreliable).
    pub fn success_rate(&self) -> f64 {
        if self.total_invocations == 0 {
            return 1.0;
        }
        self.successful_invocations as f64 / self.total_invocations as f64
    }

    /// Mean observed latency in milliseconds, or `None` when never observed.
    pub fn avg_latency_ms(&self) -> Option<f64> {
        if self.total_invocations == 0 {
            return None;
        }
        Some(self.total_latency_ms as f64 / self.total_invocations as f64)
    }

    /// A provider is selectable when it is not in cooldown.
    pub fn is_available(&self) -> bool {
        !self.is_in_cooldown()
    }
}

impl Default for ProviderHealthRecord {
    fn default() -> Self {
        Self::new(String::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityMatrix {
    pub supports_vision: bool,
    pub supports_tool_calling: bool,
    pub supports_structured_output: bool,
    pub max_context_tokens: u32,
}

impl Default for CapabilityMatrix {
    fn default() -> Self {
        Self {
            supports_vision: false,
            supports_tool_calling: true,
            supports_structured_output: false,
            max_context_tokens: 8192,
        }
    }
}

impl CapabilityMatrix {
    /// Whether this provider satisfies every capability the request requires.
    /// Used to reject routes incompatible with required tool/media support.
    pub fn satisfies(&self, required: &RequiredCapabilities) -> bool {
        (!required.vision || self.supports_vision)
            && (!required.tool_calling || self.supports_tool_calling)
            && (!required.structured_output || self.supports_structured_output)
            && self.max_context_tokens >= required.min_context_tokens
    }
}

/// The capabilities a request requires from a route.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredCapabilities {
    pub vision: bool,
    pub tool_calling: bool,
    pub structured_output: bool,
    /// Minimum context window; `0` means "no minimum".
    pub min_context_tokens: u32,
}

/// One selectable route: a provider plus its capability and health records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteCandidate {
    pub provider_name: String,
    pub capabilities: CapabilityMatrix,
    pub health: ProviderHealthRecord,
}

/// The explainable outcome of route selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDecision {
    /// Provider that was requested.
    pub requested_provider: String,
    /// Provider selected to serve the request.
    pub served_provider: String,
    /// `true` when the served provider differs from the requested one.
    pub used_fallback: bool,
}

/// Select a serving route: drop capability-incompatible and cooled-down
/// providers, then prefer the most reliable (lowest consecutive failures,
/// highest success rate) and, on ties, the lowest average latency. Returns
/// `None` when no candidate is eligible — callers must treat that as a
/// fail-closed refusal, never a silent capability downgrade.
///
/// `requested` is only used for fallback attribution; it need not be a
/// candidate.
pub fn select_route(
    candidates: &[RouteCandidate],
    requested: &str,
    required: &RequiredCapabilities,
) -> Option<RouteDecision> {
    let mut eligible: Vec<&RouteCandidate> = candidates
        .iter()
        .filter(|c| c.health.is_available() && c.capabilities.satisfies(required))
        .collect();

    eligible.sort_by(|a, b| {
        a.health
            .consecutive_failures
            .cmp(&b.health.consecutive_failures)
            .then_with(|| {
                b.health
                    .success_rate()
                    .partial_cmp(&a.health.success_rate())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                let a_lat = a.health.avg_latency_ms().unwrap_or(f64::MAX);
                let b_lat = b.health.avg_latency_ms().unwrap_or(f64::MAX);
                a_lat
                    .partial_cmp(&b_lat)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.provider_name.cmp(&b.provider_name))
    });

    let served = eligible.first()?;
    Some(RouteDecision {
        requested_provider: requested.to_string(),
        served_provider: served.provider_name.clone(),
        used_fallback: served.provider_name != requested,
    })
}

/// Process-wide live provider health registry, fed at the provider-call
/// boundary so `/api/providers/health` reports true status/latency.
static HEALTH: OnceLock<Mutex<HashMap<String, ProviderHealthRecord>>> = OnceLock::new();

fn health_registry() -> &'static Mutex<HashMap<String, ProviderHealthRecord>> {
    HEALTH.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record one provider call outcome (retained for the process lifetime).
pub fn record_provider_outcome(provider: &str, success: bool, latency_ms: u64) {
    if provider.trim().is_empty() {
        return;
    }
    let Ok(mut map) = health_registry().lock() else {
        return;
    };
    let entry = map
        .entry(provider.to_string())
        .or_insert_with(|| ProviderHealthRecord::new(provider.to_string()));
    if success {
        entry.record_success_with_latency(latency_ms);
    } else {
        entry.record_failure_with_latency(PROVIDER_COOLDOWN_MINUTES, latency_ms);
    }
}

/// Snapshot the live provider health records.
pub fn provider_snapshot() -> Vec<ProviderHealthRecord> {
    health_registry()
        .lock()
        .map(|map| map.values().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_registry_records_outcomes_and_cooldown() {
        let name = format!("openai.test-{}", uuid::Uuid::new_v4());
        record_provider_outcome(&name, true, 120);
        record_provider_outcome(&name, false, 80);
        let records = provider_snapshot();
        let rec = records
            .iter()
            .find(|r| r.provider_name == name)
            .expect("recorded provider present");
        assert_eq!(rec.total_invocations, 2);
        assert_eq!(rec.successful_invocations, 1);
        assert_eq!(rec.consecutive_failures, 1);

        // Empty names are ignored.
        record_provider_outcome("", true, 1);
        assert!(!provider_snapshot().iter().any(|r| r.provider_name.is_empty()));
    }

    fn candidate(
        name: &str,
        capabilities: CapabilityMatrix,
        failures: u32,
        successes: u64,
        latency_ms: u64,
    ) -> RouteCandidate {
        let mut health = ProviderHealthRecord::new(name.to_string());
        health.consecutive_failures = failures;
        health.successful_invocations = successes;
        health.total_invocations = successes + failures as u64;
        if successes > 0 {
            health.total_latency_ms = latency_ms;
            health.last_latency_ms = Some(latency_ms);
        }
        RouteCandidate {
            provider_name: name.to_string(),
            capabilities,
            health,
        }
    }

    #[test]
    fn latency_and_reliability_are_recorded() {
        let mut health = ProviderHealthRecord::new("p".into());
        assert_eq!(health.success_rate(), 1.0, "unproven is not unreliable");
        assert!(health.avg_latency_ms().is_none());

        health.record_success_with_latency(100);
        health.record_success_with_latency(300);
        assert_eq!(health.success_rate(), 1.0);
        assert_eq!(health.avg_latency_ms(), Some(200.0));
        assert_eq!(health.last_latency_ms, Some(300));

        health.record_failure_with_latency(30, 50);
        assert!((health.success_rate() - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(health.avg_latency_ms(), Some(150.0));
    }

    #[test]
    fn cooldown_only_after_threshold() {
        let mut health = ProviderHealthRecord::new("p".into());
        health.record_failure(30);
        health.record_failure(30);
        health.record_failure(30);
        assert!(!health.is_in_cooldown(), "<= threshold stays available");
        health.record_failure(30);
        assert!(health.is_in_cooldown());
        assert!(!health.is_available());
        health.record_success();
        assert!(health.is_available(), "success clears cooldown");
    }

    #[test]
    fn capability_matrix_rejects_incompatible_required_capabilities() {
        let no_vision = CapabilityMatrix::default();
        let vision = CapabilityMatrix {
            supports_vision: true,
            ..CapabilityMatrix::default()
        };
        let wants_vision = RequiredCapabilities {
            vision: true,
            ..Default::default()
        };
        assert!(!no_vision.satisfies(&wants_vision));
        assert!(vision.satisfies(&wants_vision));

        let wants_big = RequiredCapabilities {
            min_context_tokens: 200_000,
            ..Default::default()
        };
        assert!(!vision.satisfies(&wants_big));
    }

    #[test]
    fn select_route_rejects_incompatible_and_reports_fallback() {
        let plain = candidate("plain", CapabilityMatrix::default(), 0, 10, 100);
        let vision = candidate(
            "vision",
            CapabilityMatrix {
                supports_vision: true,
                ..CapabilityMatrix::default()
            },
            0,
            10,
            200,
        );
        let required = RequiredCapabilities {
            vision: true,
            ..Default::default()
        };

        // Requested provider is incompatible -> a compatible fallback serves.
        let decision = select_route(&[plain, vision], "plain", &required).unwrap();
        assert_eq!(decision.served_provider, "vision");
        assert!(decision.used_fallback);
        assert_eq!(decision.requested_provider, "plain");

        // No compatible candidate -> fail closed, no silent downgrade.
        let plain_only = candidate("plain", CapabilityMatrix::default(), 0, 10, 100);
        assert!(select_route(&[plain_only], "plain", &required).is_none());
    }

    /// P1.6: a fallback must preserve the request's effective capability
    /// envelope — a less-capable provider is never chosen just because it is
    /// more reliable or faster, and the decision names the served provider.
    #[test]
    fn fallback_preserves_required_capabilities_over_reliability() {
        let required = RequiredCapabilities {
            vision: true,
            tool_calling: true,
            structured_output: true,
            min_context_tokens: 1000,
        };
        let full = CapabilityMatrix {
            supports_vision: true,
            supports_tool_calling: true,
            supports_structured_output: true,
            max_context_tokens: 2000,
        };
        // The requested provider is very reliable, fast, and cheap but lacks
        // vision; the compatible fallback is slower and less proven.
        let incompatible = candidate("cheap-fast", CapabilityMatrix::default(), 0, 100, 5);
        let compatible = candidate("capable", full, 0, 1, 500);

        let decision = select_route(&[incompatible, compatible], "cheap-fast", &required).unwrap();
        assert_eq!(decision.served_provider, "capable");
        assert!(decision.used_fallback);
        assert_eq!(decision.requested_provider, "cheap-fast");
    }

    #[test]
    fn select_route_skips_cooldown_and_prefers_reliability_then_latency() {
        let mut cooled = candidate("cooled", CapabilityMatrix::default(), 0, 10, 10);
        cooled.health.record_failure(30);
        cooled.health.record_failure(30);
        cooled.health.record_failure(30);
        cooled.health.record_failure(30);
        assert!(cooled.health.is_in_cooldown());

        let flaky = candidate("flaky", CapabilityMatrix::default(), 2, 5, 50);
        let solid = candidate("solid", CapabilityMatrix::default(), 0, 8, 150);

        let decision = select_route(
            &[cooled, flaky, solid],
            "solid",
            &RequiredCapabilities::default(),
        )
        .unwrap();
        // Cooldown is skipped; the reliable provider wins even if slower.
        assert_eq!(decision.served_provider, "solid");
        assert!(!decision.used_fallback);

        // Equal failure counts -> lower average latency wins.
        let fast = candidate("fast", CapabilityMatrix::default(), 0, 5, 20);
        let slow = candidate("slow", CapabilityMatrix::default(), 0, 5, 900);
        let decision =
            select_route(&[slow, fast], "x", &RequiredCapabilities::default()).unwrap();
        assert_eq!(decision.served_provider, "fast");
        assert!(decision.used_fallback);
    }
}
