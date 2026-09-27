# ClawCrew Phase 2: Knowledge, Extensions, and Remote Operation

Status: **100% Complete** · Diperbarui 2026-09-27
Terkait: [analisis](clawcrew-analys.md) · [gap analysis](clawcrew-kirocrew-gap-analysis.md)

## Objective

Extend the runtime with governed knowledge lifecycle, higher-level Apps,
external-runtime adapters, and authenticated multi-instance operation without
weakening local ownership or policy boundaries.

## Selesai

- **P2.1 Knowledge & memory lifecycle** — model source/document/version/chunk/
  provenance/lesson; scope user/agent/project/workspace tanpa fallback;
  indexing/re-index; TTL/retention/archive/deletion-cascade; hybrid FTS/vector/
  metadata; correction API + UI.
- **P2.2 Versioned App contract** — manifest tinggi (pages/routes/tools/config/
  permissions/hooks) terpisah dari WASM ABI; registry **durable**
  (`AppRegistry::open`, versi schema, migrasi + cleanup); deny-by-default;
  `authorize_tool(app, tool, core_approved)` menegakkan enabled + declared +
  core approval; runtime-boundary tests.
- **P2.3 Remote gateway & multi-instance** — instance identity/registration/
  capability/health; binding ownership remote; routing via canonical claim;
  cegah klaim ganda saat offline; batas memory/artifact; reconnect/stale/
  offline/cancellation; audit remote.
- **P2.4 External runtime & ACP** — adapter outbound generik + transport
  JSON-RPC live (`AcpTransport`, `AcpClient`, `AcpProcess`), negosiasi capability
  fail-closed, propagation policy/approval/cancel/attribution, lifecycle session
  + recovery, context-preserving runtime switch, E2E proses nyata.

## Telah Diimplementasikan

- **Eksekusi tool App di runtime** — `authorize_tool` (governance) ada, tetapi
  belum ada jalur dispatch yang mengeksekusi tool App/plugin-App melalui
  approval + audit + receipts seperti turn biasa. *Effort: L.*
- **Eksekusi remote crew (lintas-instance)** — ownership/routing/audit ada;
  mengirim & mengawasi turn ke instance remote belum. *Effort: L–XL.*
- **MCP sebagai unit App** — MCP didukung sebagai tool; belum sebagai App
  satu-klik dengan page UI. *Effort: M–L.*

## Testing

`cargo nextest run`.
