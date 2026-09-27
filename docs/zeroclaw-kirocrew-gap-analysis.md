# ZeroClaw and Kiro Crew: Gap Analysis

Status: diperbarui 2026-09-27 · Branch: `feat/support-9router`
Referensi Kiro Crew: <https://github.com/kirodotdev/KiroCrew>
Terkait: [analisis](zeroclaw-analys.md) · [UI parity](kirocrew-ui-parity.md)

## Tujuan

Membandingkan ZeroClaw (kondisi sekarang) dengan Kiro Crew untuk menemukan
**gap yang masih relevan** menuju paritas *autonomous development workspace*.
Ini bukan rekomendasi menyalin Kiro Crew atau bergantung pada `kiro-cli`.

## Perbandingan kapabilitas (kondisi sekarang)

| Area | ZeroClaw sekarang | Kiro Crew | Gap |
|---|---|---|---|
| Runtime | Rust native, provider langsung, banyak channel/tool/hardware | Python gateway + ACP harness (default `kiro-cli`) | ZeroClaw unggul di provider langsung; tetap mandiri |
| Durable work | Satu `control_plane.db`; cron/SOP/delegasi/subagent/A2A terdaftar | TaskRunner + durable task state | ~setara |
| TaskRunner | spec + step + checkpoint terverifikasi + retry + ledger | task spec, checkpoint, retry | ~setara |
| Session manager | lifecycle, owner/generation, resume/reset/destroy, idle expiry | warm pool, resume, compaction, circuit breaker | **warm pool/reuse belum** |
| Delegation | tree durable, progress, exactly-once, governor | parallel subagents, ownership, recovery | ~setara |
| Provider routing | health/cooldown/latency **direkam live**; `select_route` ada | routing berbasis cost/latency/reliability | ~setara |
| Memory & knowledge | scope, provenance, TTL, hybrid search, correction UI | memory/lessons/knowledge search | ~setara |
| Security/audit | autonomy, approval, tool receipts, Merkle audit chain, feed | PreToolUse gate, HMAC audit chain | ~setara |
| Extensions | WASM/WIT (perm/egress/signature), App manifest + trust | MCP, Markdown skills, installable Apps | **eksekusi tool App di runtime belum** |
| Dashboard | chat, sessions, tasks, approvals, providers, audit, costs, SOP | development workspace: sessions, tasks, Apps, health, approvals, metrics, recovery | **UI Apps / Instances / recovery console belum** |
| Remote | instance identity, ownership, reconnect/offline/cancel, audit | remote crews, cross-instance access | **eksekusi lintas-instance belum** |
| Deployment | binary, service, Docker, Tauri, web | signed wheel, desktop, Docker, remote | ZeroClaw setara/kurang di signed update |

## Kelebihan ZeroClaw

1. **9router local provider** - dukungan built-in untuk local LLM router.
2. **Desktop service lifecycle** - toggle service daemon yang dikelola dari Tauri.
3. **Browser support** - kapabilitas browser natif terintegrasi.

## Gap yang masih relevan (prioritas)

1. **Route selection live (P1.5)** — `select_route`/`ProviderHealthRecord` ada
   di `zeroclaw-runtime`, tetapi dispatch provider live (di `zeroclaw-providers`)
   belum membacanya (arah dependency runtime→providers). Solusi: pindahkan
   kontrak reliability ke crate bawah (`zeroclaw-api`/`zeroclaw-config`) dan
   teruskan health ke `ReliableModelProvider` saat membangun kandidat.
   *Effort: L, lintas-crate, terisolasi.*

2. **Warm session pool / proses reuse** — Kiro Crew memelihara proses hangat dan
   pool session untuk turnaround cepat. ZeroClaw menjalankan turn per-request.
   Solusi: pool session per-agent + reuse runtime actor, dengan idle reaping
   (session idle-expiry sudah ada sebagai fondasi). *Effort: XL.*

3. **Eksekusi tool App di runtime** — `AppRegistry::authorize_tool` menegakkan
   governance, tetapi belum ada jalur dispatch yang mengeksekusi tool App/plugin
   melalui approval + audit + receipts seperti turn biasa. *Effort: L.*

4. **Eksekusi remote crew (lintas-instance)** — ownership/routing/audit ada;
   mengirim dan mengawasi turn ke instance remote belum. *Effort: L–XL.*

5. **Paritas UI (development workspace)** — halaman **Apps**, **Instances/
   Crews**, **recovery console terpadu**, dan **diff/editor** melekat belum ada.
   Detail & rencana: [kirocrew-ui-parity.md](kirocrew-ui-parity.md). *Effort:
   UI berlapis, dapat dipecah.*

6. **MCP sebagai unit App** — MCP didukung sebagai tool/deferred; belum sebagai
   App satu-klik dengan page UI. *Effort: M–L.*

## Yang sebaiknya tidak ditiru

Ketergantungan wajib `kiro-cli`, rewrite gateway ke Python, format data home
Kiro Crew, jumlah fitur sebagai tujuan, App in-process tanpa trust boundary,
remote default-on.

## Definisi selesai untuk paritas

Sebuah kapabilitas disebut setara bila punya: sumber kebenaran bernama, batas
lifecycle & ownership terdokumentasi, perilaku security/privacy eksplisit,
proyeksi API + dashboard, perilaku restart/cancel/rollback, serta tes fokus di
batas perilaku nyata.
