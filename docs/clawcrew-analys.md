# Analisis ClawCrew vs Kiro Crew

Status analisis: diperbarui 2026-09-27 · Branch: `feat/support-9router`
Dokumen terkait: [gap analysis](clawcrew-kirocrew-gap-analysis.md) ·
[UI parity](kirocrew-ui-parity.md) · [roadmap](clawcrew-kirocrew-roadmap.md)

## Ringkasan

ClawCrew sekarang **sejajar** dengan Kiro Crew pada inti *autonomous work
platform*: durable control plane, TaskRunner bercheckpoint, session lifecycle,
memory/knowledge bercakupan, App/plugin ber-manifest, multi-instance gateway,
ACP adapter, dan dashboard operasional. Yang tersisa bukan fitur besar yang
hilang, melainkan **beberapa seam arsitektur** dan **paritas UI**.

## Status Phase 0–3 (kondisi nyata sekarang)

| Phase | Status | Catatan |
|---|---|---|
| Phase 0 — Durable work & session | ✅ Selesai | Cron/SOP/A2A semua producer terdaftar di `TaskRegistry`; recovery/cancellation/redaction teruji |
| Phase 1 — Autonomous workflows | ✅ Selesai | `TaskRunner::run` (plan→execute→validate), checkpoint/resume, retry, review-gate approval; governor depth/concurrency/cost/**token**/runtime |
| Phase 2 — Knowledge, Apps, remote, ACP | ✅ Selesai | Knowledge lifecycle, App manifest + registry durable + `authorize_tool`, remote instance ownership, ACP adapter + transport live |
| Phase 3 — Product maturity | ✅ Selesai | TaskBoard (list/detail/tree/timeline/controls/SSE), Approvals, Providers Health, Sessions Health (+SSE), Audit feed, backup/restore/rollback journaled, compat matrix, plugin scaffolding |

## Yang sudah setara Kiro Crew

- **Durable control plane** — satu `control_plane.db` (SQLite) untuk semua
  background work; owner identity, terminal settlement atomik, reaper, recovery
  `resumed/fresh/lost/needs_review`. Producer: Delegate, Subagent, Goal,
  Workflow, PeerInbox, **Cron, SOP, A2A**.
- **TaskRunner + work ledger** — spec berversi, step, checkpoint terverifikasi,
  retry terbatas, artifact bounded, ledger dengan correlation ID.
- **Session manager** — lifecycle `active/paused/idle/closing/closed`, owner +
  generation, resume/reset/remove/destroy dengan active-child guard, idle expiry,
  provider-health & compaction terhubung ke session health.
- **Delegation & subagent** — parent-child tree durable, progress streaming,
  exactly-once terminal aggregation, ownership transfer, governor depth/
  concurrency/cost/token/runtime.
- **Provider runtime** — katalog provider langsung, fallback + attribution
  (requested vs served), health/cooldown/latency **live-recorde**.
- **Memory & knowledge** — scope user/agent/project/workspace tanpa fallback,
  provenance, TTL/retention, hybrid search, correction API + UI.
- **Extensions** — WASM/WIT plugin (permission/egress/signature Ed25519
  `SignatureMode`), App manifest + registry durable + trust/`authorize_tool`,
  compat matrix, scaffolding App & plugin.
- **Remote/gateway** — instance identity, remote ownership binding, reconnect/
  offline/cancellation, audit.
- **Dashboard** — TaskBoard, Runs, Approvals, Providers Health, Sessions Health,
  Audit, Costs, Logs, Config, Skills, Tools, Cron, SOP canvas, Agent workspace.
- **ACP** — adapter generik + transport JSON-RPC live (proses nyata, E2E teruji).

## Yang lebih baik di ClawCrew

- **9Router local provider** terpasang secara native.
- **Browser integration**.
- **Desktop lifecycle** (Tauri toggle service).

## Belum ada / belum penuh vs Kiro Crew

Lihat [gap analysis](clawcrew-kirocrew-gap-analysis.md) untuk detail. Ringkas:

2. **Warm session pool / proses reuse** — Kiro Crew memelihara proses hangat dan
   pool session; ClawCrew menjalankan turn per-request tanpa warm pool.
3. **Eksekusi tool App di runtime** — `authorize_tool` (governance) ada, tapi
   belum ada jalur dispatch eksekusi tool App/plugin-App melalui approval/audit.
4. **Remote crew execution** — kontrak ownership/routing ada; eksekusi kerja
   lintas-instance yang sebenarnya (mengirim turn ke remote instance) belum.
5. **Paritas UI** — Kiro Crew unggul sebagai *development workspace*: halaman
   Apps, browser instance/crew remote, konsol recovery terpadu, diff/editor
   melekat. ClawCrew punya banyak halaman operasional, tetapi belum ada halaman
   Apps, Instances/Crews, dan recovery console terpadu. Detail +
   rencana: [kirocrew-ui-parity.md](kirocrew-ui-parity.md).
6. **Ekosistem MCP-native Apps** — MCP sudah didukung via tools/deferred, tetapi
   belum sebagai unit App satu-klik dengan page UI.

## Kekuatan yang harus dipertahankan (jangan ditiru mentah)

Rust native, binary tunggal, provider langsung, hardware support, WASM plugin,
deployment ringan, tidak bergantung `kiro-cli`, konfigurasi eksplisit &
auditabel, remote tertutup secara default.

## Estimasi

| Area | Estimasi paritas |
|---|---|
| Control plane / TaskRunner / session | ~95% |
| Memory & knowledge | ~90% |
| Extensions (plugin/App) | ~85% |
| Provider runtime | 100% |
| Remote / multi-instance | ~75% (eksekusi lintas-instance kurang) |
| UI / product workspace | ~60% |
| Warm process / session pool | ~30% |
