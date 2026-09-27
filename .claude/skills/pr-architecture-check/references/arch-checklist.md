# Architecture Checklist — PR Review Criteria

This checklist is the reference for the `pr-architecture-check` skill. Apply
each category to the PR diff. Skip categories that are irrelevant to the
files changed.

---

## 1. Dependency Direction

> "Dependencies flow inward. The runtime knows nothing about the plugins.
> Plugins know about the API. Nothing knows about everything."
> — FND-001 §4.1

- Imports must flow inward toward `clawcrew-api`
- `clawcrew-api` must have no dependencies on runtime, tools, channels, providers, or any implementation crate
- Implementation crates (`clawcrew-channels`, `clawcrew-tools`, `clawcrew-providers`, `clawcrew-memory`) depend on `clawcrew-api` — not on each other
- `clawcrew-runtime` depends on `clawcrew-api` and foundation crates — it has no knowledge of specific channel, tool, or provider implementations
- Plugins depend on `clawcrew-api` (not the runtime)
- New `use` / `extern crate` / `Cargo.toml` dependency additions that point outward (from API toward implementations) are violations

**Check:** Review `Cargo.toml` changes and `use` statements in the diff. Flag any dependency that flows outward (from a lower-layer crate toward a higher-layer one).

---

## 2. Trait Boundary Compliance

> Core architecture is trait-driven and modular. — AGENTS.md

- New functionality that crosses a trait boundary must go through the trait, not around it
- No hardcoding around trait boundaries (e.g., matching on a specific provider name instead of using the `Provider` trait interface)
- No type-casting or downcasting to bypass a trait abstraction
- Extension points are defined in `clawcrew-api`:
  - `Provider` (`crates/clawcrew-api/src/provider.rs`)
  - `Channel` (`crates/clawcrew-api/src/channel.rs`)
  - `Tool` (`crates/clawcrew-api/src/tool.rs`)
  - `Memory` (`crates/clawcrew-api/src/memory_traits.rs`)
  - `Observer` (`crates/clawcrew-api/src/observability_traits.rs`)
  - `RuntimeAdapter` (`crates/clawcrew-api/src/runtime_traits.rs`)
  - `Peripheral` (`crates/clawcrew-api/src/peripherals_traits.rs`)

**Check:** Look for string matching on implementation names, `downcast_ref`, concrete type assertions, or `match` arms that enumerate specific implementations where a trait method should be used.

---

## 3. Extension Pattern Conformance

> Extend by implementing traits and registering in factory modules. — AGENTS.md

- New providers, channels, tools, memory backends, observers, and peripherals must follow the factory registration pattern
- New implementations should:
  1. Implement the relevant trait from `clawcrew-api`
  2. Register in the corresponding factory module
  3. Be discoverable via configuration, not hardcoded into the runtime
- Adding a new implementation should not require modifying the runtime or other existing implementations

**Check:** If the PR adds a new implementation of an extension point, verify it registers via the factory pattern and does not require changes to unrelated crates.

---

## 4. Crate Responsibility (Placement)

> Each crate has a defined responsibility. — AGENTS.md Repository Map

New code must land in the correct crate per the repository map:

| Crate | Responsibility |
|---|---|
| `clawcrew-api` | Public trait definitions only — no implementations, no heavy dependencies |
| `clawcrew-config` | Schema, config loading/merging |
| `clawcrew-macros` | `Configurable` derive macro |
| `clawcrew-providers` | Model provider implementations and resilient wrapper |
| `clawcrew-channels` | Messaging platform integrations, orchestrator, media pipeline |
| `clawcrew-tools` | Tool execution surface (shell, file, memory, browser) |
| `clawcrew-runtime` | Agent loop, security, cron, SOP, skills, observability |
| `clawcrew-memory` | Memory backends (markdown, sqlite, embeddings, vector merge) |
| `clawcrew-infra` | Shared infrastructure (debounce, session, stall watchdog) |
| `clawcrew-gateway` | Webhook/gateway server (separate binary) |
| `clawcrew-hardware` | USB discovery, peripherals, serial, GPIO |
| `clawcrew-tui` | TUI onboarding wizard |
| `clawcrew-plugins` | WASM plugin system |
| `clawcrew-tool-call-parser` | Tool call parsing |

**Check:** Verify new modules/files are placed in the crate whose responsibility matches the functionality. Flag code that belongs in one crate but is placed in another.

---

## 5. Core Engineering Constraints

These 7 constraints from AGENTS.md are non-negotiable. A PR that violates one
should be flagged.

### 5.1 Single static binary

The project ships as a single static binary. Flag changes that introduce
mandatory runtime dependencies, require external services for core
functionality, or cause significant binary size growth without proportional
value.

### 5.2 Trait-driven pluggability

All extension points use traits. Flag changes that bypass or hardcode around
trait boundaries.

### 5.3 Minimal footprint

Target is <5 MB binary, minimal RAM/CPU. Flag changes that add significant
overhead (heavy dependencies, unbounded caches, expensive default paths).

### 5.4 Runs on anything (RPi Zero hardware floor)

Must run on edge targets including Raspberry Pi Zero. Flag changes that
require hardware or OS features unavailable on constrained devices.

### 5.5 Secure by default

Deny-by-default security posture. Flag changes that weaken security policy,
broaden the attack surface, or add default-allow rules.

### 5.6 No vendor lock-in

No provider gets privilege outside the trait boundary. Flag changes that
grant special treatment to a specific vendor or provider.

### 5.7 Zero external infrastructure

Core functionality must work without third-party services. Flag changes that
make an external service a hard dependency for core features.

---

## Usage Notes

- **Pass**: No issues found for this category.
- **Advisory**: Potential concern — worth reviewer attention but may be intentional.
- **Flag**: Likely violation of a documented constraint — reviewer should evaluate.

Only report categories relevant to the PR diff. A docs-only PR needs no
dependency direction check.
