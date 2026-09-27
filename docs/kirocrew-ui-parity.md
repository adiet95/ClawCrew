# ZeroClaw UI Parity with Kiro Crew

Status: rencana · Diperbarui 2026-09-27 · Branch: `feat/support-9router`
Terkait: [analisis](zeroclaw-analys.md) · [gap analysis](zeroclaw-kirocrew-gap-analysis.md)

## Tujuan

Menjadikan dashboard ZeroClaw **development workspace** seperti Kiro Crew:
satu tempat untuk melihat dan mengendalikan session, task, Apps, health,
approval, metrics, dan recovery — bukan hanya konfigurasi.

Prinsip (ponytail): pakai komponen yang sudah ada (`Card`, `Badge`, `Button`,
`PageHeader`, `SSEClient`, `apiFetch`), dan **endpoint gateway yang sudah ada**
sebelum menambah backend baru.

## Halaman ZeroClaw yang sudah ada

`Dashboard`, `AgentsList`, `AgentChat`, `AgentWorkspaceExplorer`, `ChatWorkspace`,
`Tools`, `Skills`, `Sops`, `SopCanvas`, `Runs`, `RunDetail`, `TaskBoard`,
`Approvals`, `ProvidersHealth`, `SessionsHealth`, `Audit`, `Logs`, `Cron`,
`Integrations`, `Pairing`, `Doctor`, `Canvas`, `Config`.

## Peta permukaan Kiro Crew → ZeroClaw

| Permukaan Kiro Crew | Rute ZeroClaw | Status |
|---|---|---|
| Sessions (health, resume, compaction) | `/sessions` | ✅ (Sessions Health + SSE) |
| Tasks (board, checkpoint, controls) | `/tasks` | ✅ (TaskBoard: tree, timeline, resume/retry/cancel) |
| Approvals | `/approvals` | ✅ |
| Health / provider health | `/providers` | ✅ |
| Metrics / cost per agent | `/` (Dashboard → Costs tab) | ✅ sebagian |
| Audit / security feed | `/audit` | ✅ |
| SOP runs | `/runs` | ✅ |
| **Apps** (install/enable/update/rollback, pages, tools) | — | ❌ belum ada |
| **Instances / remote crews** | — | ❌ belum ada |
| **Recovery console terpadu** (lost/needs_review lintas entitas) | — | ❌ belum ada |
| **Diff/editor melekat** | `/canvas` (terbatas) | 🟡 sebagian |
| Knowledge / memory inspection | `/` (Dashboard → Memory tab) | ✅ sebagian |
| **Warm-session / pool management** | — | ❌ (fitur runtime belum ada) |

## Rencana UI (dapat dipecah per PR)

### UI.1 — Apps page (`/apps`)
- Sumber data: `AppRegistry` (durable) yang perlu diproyeksikan via gateway
  (mis. `GET /api/apps`, `POST /api/apps/:id/enable|disable|update|rollback`,
  `DELETE /api/apps/:id`), membungkus `session`-scoped registry.
- Tampilkan: manifest (name, version, min_runtime_version, dependencies, tools,
  permissions), state (installed/enabled/disabled), tombol enable/disable/
  update/rollback/remove (remove disabled saat enabled).
- Governance: tampilkan verdict `authorize_tool` per tool (deny-by-default).
- Backend baru diperlukan (proyeksi Apps + endpoints). *Effort: M.*

### UI.2 — Instances / Crews page (`/instances`)
- Sumber data: P2.3 instance registry (`zeroclaw_api::instance`) via gateway.
- Tampilkan: instance id, capabilities, health, reconnect/offline, ownership,
  audit remote.
- Backend proyeksi kemungkinan sudah ada di kontrak P2.3. *Effort: M.*

### UI.3 — Unified recovery console (`/recovery`)
- Sumber data: task store `NeedsReview`/`Lost`/`TimedOut` + settlement intents.
- Aksi: review, retry (reopen), cancel, acknowledge — memakai endpoint yang
  sudah ada (`/api/dashboard/tasks/:id/reopen|cancel`) dan menambah filter status.
- Bisa jadi tab pada `/tasks` alih-alih halaman baru. *Effort: S–M.*

### UI.4 — Diff/editor & metrics polish
- Canvas/diff: tingkatkan `/canvas` atau tambah diff viewer untuk artifact &
  perubahan file (memakai endpoint workspace/browse yang ada).
- Metrics: satukan cost/token/latency/fallback per task & agent dalam satu view
  (memakai `/api/dashboard/tasks/stats` + CostTracker). *Effort: M.*

### UI.5 — Live activity di seluruh halaman
- Ganti polling yang tersisa dengan `SSEClient` (task & session SSE sudah ada).
  *Effort: S.*

## Catatan desain

- Navigasi sudah dikelompokkan **Configure vs Operations** — pertahankan.
- Indikator live/reconnect konsisten dengan TaskBoard & Sessions Health.
- Semua aksi destruktif harus mencerminkan guard backend (mis. App enabled
  tidak boleh di-remove; task terminal tidak boleh di-reopen).

## Definisi selesai UI

Setiap halaman: punya sumber data kanonik, aksi tertutup default, indikator
live bila relevan, keadaan kosong/error yang jelas, dan tes logika murni
(`node --test`) untuk parsing/transformasi.
