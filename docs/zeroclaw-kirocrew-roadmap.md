# ZeroClaw Future Capability Roadmap

Status: **Phase 0–3 complete** (UI parity remaining - backend projection incomplete) · Audit update: 2026-09-27 · Related: [analysis](zeroclaw-analys.md) · [gap analysis](zeroclaw-kirocrew-gap-analysis.md) · [UI parity](kirocrew-ui-parity.md)

## Goal

Expand ZeroClaw from a capable local agent runtime into a dependable autonomous work platform while preserving its core advantages:

- Native Rust implementation.
- Direct and provider-agnostic model access.
- Small, deployable binaries.
- Strong local ownership of data and credentials.
- Broad channels, hardware, tools, and plugin surfaces.

This roadmap is capability-oriented. Each item should become a separately tracked issue or PR after the design boundary is accepted.

## Priority Model

- **P0:** Foundation. Missing this creates correctness or recovery risk.
- **P1:** High-value autonomous workflow capability.
- **P2:** Platform and ecosystem capability.
- **P3:** Product maturity and scale.

## Phase 0 / P0: Durable Work and Session Foundation

**Current status:** Complete (2026-09-27). All background producers (cron, SOP,
A2A) register and settle through the canonical control plane; session lifecycle,
recovery, events, and boundary tests are done. See
[Phase 0 tasks](zeroclaw-phase0.md).

### Phase 0 implementation snapshot

| Area | Current state | Remaining boundary |
|---|---|---|
| Durable task authority | SQLite-backed `control_plane.db` with typed task kinds/statuses, owner PID and boot identity, session/workspace fields, cancellation state, child lookup, and task events | Cron, SOP, and goal-mode lifecycles still have separate owners; migrate them deliberately rather than duplicating state |
| Claims and terminal transitions | Atomic claim, owner-checked terminal settlement, compare-and-set cancellation, idempotency fields, and terminal output snapshots are present | Verify every producer uses the same transition path and define downgrade behavior for older records |
| Crash recovery | Startup recovery and periodic reaper can reconcile abandoned work as `lost` or stale-heartbeat work as `timed_out`; artifact settlement is digest-checked | Recovery marks interrupted work truthfully; it does not resume execution or prove that an external side effect was rolled back |
| TaskRunner foundation | Versioned-in-practice spec types, step status, validation gates, retry policy, and SQLite work ledger exist | Wire plan/execute/validate/pause/resume/cancel orchestration and bounded side-effect retry into the runtime |
| Session contract | Initial `Active`/`Paused`/`Idle`/`Closing`/`Closed` lifecycle and provider-health fields exist | Persist ownership/generation, resume/reset/destroy decisions, compaction, idle expiry, and orphan cleanup at the session boundary |
| Events and operations | Typed runtime observability and durable per-task events already exist; dashboard response types and an initial Task Board are present | Complete event projections, correlation/redaction guarantees, live subscriptions, and real task/session health endpoints |

**Phase 0 exit criteria:** all background-work producers either use the canonical
control plane or explicitly document why they cannot; restart outcomes are
observable as `resumed`, `lost`, or `needs_review`; task/session projections read
from their owning stores; and boundary tests cover duplicate terminal settlement,
owner identity, cancellation, artifact recovery, and redaction.

### P0.1 Unified task control plane

**Outcome:** Cron jobs, SOP runs, delegated tasks, subagents, and future workflows expose one durable lifecycle contract.

**Tasks:**

- Keep the versioned task record and status enum as the canonical lifecycle contract.
- Record owner agent, origin surface, parent task, session key, workspace, timestamps, process identity, heartbeat, and cancellation state.
- Keep one durable SQLite authority; keep output artifacts as content, not lifecycle state.
- Preserve atomic claim, owner-checked terminal settlement, and compare-and-set cancellation.
- Add startup recovery for stale claims and interrupted tasks, with conservative `lost`/`timed_out` outcomes.
- Keep task event history bounded, correlated, and redacted before persistence or egress.
- Migrate cron, SOP, and goal-mode ownership only after their lifecycle and rollback contracts are explicit.
- Define compatibility and downgrade behavior for task records.

**Acceptance criteria:**

- A task cannot have two terminal outcomes.
- Restart recovery never silently re-runs an unverified side effect.
- Recovery distinguishes durable supervision from actual execution resume.
- Every background path identifies its owner and lifecycle authority.
- API and dashboard status are projections of the same store.

**Dependencies:** Config lifecycle, runtime state and persistence, background work lifecycle.

### P0.2 Session lifecycle contract

**Outcome:** Sessions have consistent admission, ownership, cancellation, resume, compaction, and cleanup behavior.

**Tasks:**

- Keep the canonical session state machine (`active`, `paused`, `idle`, `closing`, `closed`) documented and versioned.
- Unify session ownership and generation checks across user, cron, subagent, and workflow sessions.
- Add explicit resume, reset, remove, and destroy semantics, including active-child guards.
- Connect provider liveness and circuit-breaker state to session health rather than a process-local flag only.
- Add context usage and compaction decisions with observable outcomes.
- Add idle expiry and orphan process cleanup with fail-closed identity checks.
- Persist the lifecycle and expose session health through a typed API.

**Acceptance criteria:**

- A session cannot be reset while an owned turn or child task is still active.
- Restart behavior is observable as resumed, fresh, lost, or needs-review.
- A stale process cannot receive a successor session's cleanup signal.

**Dependencies:** P0.1, provider lifecycle, runtime state and persistence.

### P0.3 Event and observability contract

**Outcome:** Users and operators can observe task, session, provider, approval, and recovery transitions consistently.

**Tasks:**

- Define typed runtime events and stable event IDs.
- Publish events to logs, WebSocket subscribers, and durable task/session history as appropriate.
- Add correlation fields for agent, session, task, provider, and channel.
- Standardize redaction before persistence and egress.
- Add structured task and session health snapshot endpoints backed by the canonical stores.
- Add live dashboard subscriptions and bounded pagination for task/event history.

**Acceptance criteria:**

- A task status shown in the dashboard can be traced to a durable event.
- Sensitive prompts, credentials, and tool arguments are not written into generic events.
- Event consumers tolerate unknown future event variants.
- Health snapshots identify stale, lost, paused, and provider-degraded work without inventing a second registry.

## P1: Autonomous Development Workflows

**Current status:** Complete (2026-09-27), except routing wiring. TaskRunner is a
real engine (run/checkpoint/resume/retry/validation/approval) and the governor
suite (depth/concurrency/cost/token/runtime) is enforced. Remaining: wiring
`select_route` into live provider dispatch. See [Phase 1 tasks](zeroclaw-phase1.md).

### P1.1 TaskRunner

**Outcome:** ZeroClaw can execute a multi-step specification with checkpoints and validation, including explorative work and multi-agent coordination.

**Tasks:**

- Evolve the existing task specification, step schema, validation gates, and retry metadata into a versioned contract.
- Connect the existing SQLite work ledger to every step transition and bounded output artifact.
- Support plan, execute, validate, retry, pause, resume, and cancel states.
- Add multi-agent plan and parallel execution capabilities.
- Represent acceptance conditions as explicit validation gates (e.g., test, lint, build, review).
- Capture step inputs, bounded outputs, and artifacts.
- Prevent duplicate side effects during retry.
- Add operator approval gates for risky steps.

**Acceptance criteria:**

- A failed step can resume from its last valid checkpoint.
- A retry policy is visible and bounded.
- Validation failure prevents false task completion.
- Task state cleanly resumes after a gateway restart.

**Dependencies:** P0.1, P0.3, tool receipts, approvals.

### P1.2 Delegation and subagent supervision

The control plane already records parent-child relationships and supports
descendant cancellation. Progress streaming, ownership transfer, and complete
resource governors are still required.

**Outcome:** Parallel work is visible, cancellable, policy-bounded, and recoverable.

**Tasks:**

- Add a parent-child task graph to the control plane.
- Stream child progress and terminal results to the parent.
- Add child cancellation and bounded cleanup.
- Preserve policy, memory, action, and cost inheritance.
- Add depth, concurrency, and resource limits.
- Expose a subagent tree in the dashboard.

**Acceptance criteria:**

- Child work cannot outlive its parent without an explicit ownership transfer.
- A parent receives exactly one terminal child result.
- Child actions remain within the parent's effective security ceiling.

**Dependencies:** P0.1, P0.2, existing delegation and subagent contracts.

### P1.3 Provider reliability and routing

Provider reliability scaffolding and attribution hooks exist, but capability
compatibility, health scoring, cooldown policy, and explainable fallback are not
yet a complete routing contract.

**Outcome:** Model selection accounts for capability, reliability, cost, and latency without losing attribution.

**Tasks:**

- Define provider capability and health records.
- Add bounded health scoring and cooldown state.
- Make fallback decisions visible in task/session metadata.
- Add tool-capability compatibility checks before dispatch.
- Preserve requested-versus-served provider attribution.
- Add cost and latency aggregation per agent and task.

**Acceptance criteria:**

- Fallback never silently changes the effective policy or tool capability.
- A response identifies the served provider when fallback occurs.
- Routing data does not expose credentials or sensitive endpoint details.

## P2: Knowledge, Extensions, and Remote Operation

**Current status:** Complete (2026-09-27), with follow-ups. Knowledge lifecycle,
durable App registry + `authorize_tool`, remote ownership, and the ACP adapter
(live transport + E2E) are done. Remaining: App tool runtime execution dispatch,
remote cross-instance execution, MCP-as-App. See [Phase 2 tasks](zeroclaw-phase2.md).

### P2.1 Memory and knowledge lifecycle

**Outcome:** Conversation history, memory, lessons, and indexed knowledge have explicit ownership, retention, and granular UI controls.

**Tasks:**

- Define memory classes and scopes (user, agent, project, workspace).
- Add document and folder-level knowledge bases.
- Add provenance and source links to learned entries.
- Add retention, archive, and deletion APIs (TTL).
- Add hybrid search (FTS + vector + metadata).
- Add dashboard inspection and explicit correction controls (e.g., "delete", "forget this", "edit").
- Separate the lesson store from conversational memory.
- Prevent unavailable scoped memory from silently falling back to another scope.

**Acceptance criteria:**

- Every durable memory item has a scope and deletion path.
- Search results identify their source and confidence/provenance.
- Memory writes obey the active agent and policy boundary.

Existing memory backends and embeddings do not yet constitute the full scoped
knowledge lifecycle described here. Document indexing, provenance, retention,
correction APIs, and no-fallback scope enforcement remain explicit work.

### P2.2 Versioned App and plugin contract

The low-level WASM/WIT boundary and initial app manifest/registry types exist.
Installation, compatibility rejection, permission enforcement, rollback, and
removal behavior still need runtime-boundary tests.

**Outcome:** Extensions can add tools and UI without bypassing core trust controls, treating Apps as cohesive product units.

**Tasks:**

- Define an extension manifest and compatibility version.
- Declare routes, pages, tools, config, permissions, and lifecycle hooks.
- Keep capability grants deny-by-default.
- Route all tool execution through core approvals and governance.
- Add install, update, disable, rollback, and removal flows.
- Document the boundary between WASM/WIT plugins and higher-level Apps (Apps as product units).

**Acceptance criteria:**

- An extension cannot widen policy or bypass audit.
- Incompatible extensions are refused before activation.
- Removing an extension does not corrupt core task/session state.

### P2.3 Remote gateway and multi-instance support

Instance and dashboard data types exist, but authenticated pairing, routing,
remote ownership, reconnect behavior, and audit-backed multi-instance execution
are not implemented end to end.

**Outcome:** Operators can manage multiple ZeroClaw gateways without weakening local ownership guarantees.

**Tasks:**

- Define device identity, zero-trust mutual authentication, and device pairing.
- Add instance registration, health, and capability discovery.
- Route tasks and sessions explicitly to an instance.
- Define remote memory and artifact boundaries, with clear synchronization or routing mechanisms.
- Add reconnect, offline, and stale-instance behavior.
- Add audit records for remote actions.

**Acceptance criteria:**

- A remote client cannot access an unassigned session or artifact.
- Offline instances do not create duplicate task claims.
- Remote actions remain attributable to user, instance, agent, and task.

### P2.4 Extensible Runtime and ACP Adapters

An initial generic ACP adapter exists, but its tool invocation path is currently
a protocol-shaped stub rather than a complete policy-enforced external-runtime
integration. Context-preserving runtime switching remains planned.

**Outcome:** ZeroClaw can integrate with external agent runtimes and operate as a universal ACP client without becoming dependent on a single CLI.

**Tasks:**

- Build generic ACP adapters decoupled from IDE-specific surfaces.
- Support integration with external agent runtimes.
- Maintain seamless context preservation when switching between models and runtimes.

**Acceptance criteria:**

- ZeroClaw can delegate tasks to an external runtime via standard protocols.
- Model and provider switches do not silently discard conversation history.

## Suggested Delivery Order

1. P0.1 Unified task control plane.
2. P0.2 Session lifecycle contract.
3. P0.3 Event and observability contract.
4. P1.1 TaskRunner.
5. P1.2 Delegation and subagent supervision.
6. P1.3 Provider reliability and routing.
7. P2.1 Memory and knowledge lifecycle.
8. P2.2 Versioned App and plugin contract.
9. P2.3 Remote gateway and multi-instance support.
10. P2.4 Extensible Runtime and ACP Adapters.
11. P3.1 Task-oriented dashboard.
12. P3.2 Backup, migration, update, and rollback.
13. P3.3 Extension and provider ecosystem.

## Non-Goals

This roadmap does not require:

- Replacing Rust with Python.
- Making `kiro-cli` a required dependency.
- Copying Kiro Crew's private data formats.
- Adding features solely to match a competitor's count.
- Enabling remote access by default.
- Allowing plugins or Apps to bypass ZeroClaw's security boundary.

## Definition of Done for This Roadmap

A capability is complete only when it has:

- A named source of truth.
- A documented lifecycle and ownership boundary.
- Explicit security and privacy behavior.
- API and dashboard projections where user-facing.
- Restart, cancellation, failure, and rollback behavior.
- Focused automated tests at the real behavior boundary.
- Documentation and migration notes where persisted state changes.

## Audit update: 2026-09-27

All of Phase 0–1 checklists are complete; Phase 2–3 are complete except the
follow-ups listed in the phase docs. What remains is:
**App tool runtime execution**, **remote cross-instance execution**, and
**UI parity** (Apps/Instances/Recovery pages).

Actionable task breakdowns are tracked in:

- [Phase 2 tasks](zeroclaw-phase2.md)
- [UI parity](kirocrew-ui-parity.md)
