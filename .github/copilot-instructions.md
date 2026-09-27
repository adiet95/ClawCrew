# ClawCrew Graph-First Navigation

These instructions apply to VS Code Copilot Chat in this workspace.

## Mandatory Graphify Protocol

For every codebase question or investigation involving architecture, symbols, call sites, dependencies, paths, data flow, lifecycle, or module relationships:

1. Work from the repository root.
2. Check whether `graphify-out/graph.json` exists.
3. Index before searching:
   - If the graph exists, run `graphify update .` to refresh it from the current workspace.
   - If it does not exist, run `graphify .` (use `graphify . --no-viz` for large graphs when visualization is unnecessary).
   - On Windows PowerShell, use `graphify`, not `/graphify`.
4. Query the graph before opening source files:
   - `graphify query "..."` for architecture and relationship questions.
   - `graphify path "Source" "Target"` for a path or flow.
   - `graphify explain "SymbolOrConcept"` for one node.
5. Only after Graphify identifies the relevant files or symbols, use targeted `Select-String`/grep and read the specific file ranges needed for line-level detail.

Never begin architecture discovery with broad recursive grep, directory browsing, or guessed file paths. If a query appears to miss a recent change, run `graphify update .` again before falling back to manual inspection. Treat Graphify edges as evidence with their confidence labels; do not invent relationships absent from the graph or source.

For implementation tasks, keep the graph query result and the targeted source read aligned. After edits that change code structure, run `graphify update .` before a follow-up query or flow investigation.
