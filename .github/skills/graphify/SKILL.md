---
name: graphify
description: "Use for any question about a codebase, its architecture, file relationships, or project content — especially when graphify-out/ exists, where the question should be treated as a graphify query first. Turns any input (code, docs, papers, images, videos) into a persistent knowledge graph with god nodes, community detection, and query/path/explain tools."
---

# /graphify — VS Code / Antigravity Adaptation

Turn any folder of files into a navigable knowledge graph with community detection, an honest audit trail, and three outputs: interactive HTML, GraphRAG-ready JSON, and a plain-language GRAPH_REPORT.md.

> **Platform note:** This skill is adapted for **Windows (pwsh)** and **VS Code / Antigravity IDE**. All shell commands use PowerShell syntax. Python is invoked as `python` (not `python3`).

## Usage

```
/graphify                                             # full pipeline on current directory
/graphify <path>                                      # full pipeline on specific path
/graphify <path> --mode deep                          # thorough extraction, richer INFERRED edges
/graphify <path> --update                             # incremental - re-extract only new/changed files
/graphify <path> --directed                           # build directed graph (preserves edge direction)
/graphify <path> --cluster-only                       # rerun clustering on existing graph
/graphify <path> --no-viz                             # skip visualization, just report + JSON
/graphify <path> --wiki                               # build agent-crawlable wiki
/graphify <path> --obsidian                           # generate Obsidian vault
/graphify <path> --svg                                # also export graph.svg
/graphify <path> --graphml                            # export graph.graphml (Gephi, yEd)
/graphify <path> --neo4j                              # generate cypher.txt for Neo4j
/graphify query "<question>"                          # BFS traversal on existing graph
/graphify query "<question>" --dfs                    # DFS - trace a specific path
/graphify query "<question>" --budget 1500            # cap answer at N tokens
/graphify path "AuthModule" "Database"                # shortest path between two concepts
/graphify explain "SwinTransformer"                   # plain-language explanation of a node
```

## What You Must Do When Invoked

**Fast path — existing graph:** Before doing anything else, check whether `graphify-out/graph.json` exists:

```powershell
Test-Path graphify-out/graph.json
```

If it exists AND the user's request is a natural-language question about the codebase (e.g. "How does X work?", "What calls Y?", "Trace the data flow through Z") and NOT an explicit rebuild command (`--update`, `--cluster-only`, or a bare path that implies fresh extraction): **skip all build steps and jump straight to query mode.** Run `graphify query "<question>"` immediately.

If no path was given, use `.` (current directory). Do not ask the user for a path.

## Step 1 — Ensure graphify is installed

```powershell
# Detect Python and graphify availability
$GraphifyPython = $null

# 1. Check if graphify CLI is available
$GraphifyBin = Get-Command graphify -ErrorAction SilentlyContinue

# 2. Try python directly
if (-not $GraphifyPython) {
    try {
        python -c "import graphify" 2>$null
        if ($LASTEXITCODE -eq 0) { $GraphifyPython = "python" }
    } catch {}
}

# 3. Install if missing
if (-not $GraphifyPython) {
    python -m pip install graphifyy -q 2>$null
    $GraphifyPython = "python"
}

# 4. Write interpreter path for subsequent steps
New-Item -ItemType Directory -Path graphify-out -Force | Out-Null
python -c "import sys; open('graphify-out/.graphify_python', 'w', encoding='utf-8').write(sys.executable)"

# 5. Save scan root
$ScanRoot = (Resolve-Path INPUT_PATH).Path
Set-Content -Path graphify-out/.graphify_root -Value $ScanRoot
```

If the import succeeds, print nothing and move to Step 2.

**In every subsequent block, read the interpreter from the saved path:**

```powershell
$PyInterp = Get-Content graphify-out/.graphify_python -Raw
# Use: & $PyInterp -c "..."
```

## Step 2 — Detect files

```powershell
$PyInterp = (Get-Content graphify-out/.graphify_python -Raw).Trim()
& $PyInterp -c @"
import json
from graphify.detect import detect
from pathlib import Path
result = detect(Path('INPUT_PATH'))
Path('graphify-out/.graphify_detect.json').write_text(json.dumps(result, ensure_ascii=False), encoding='utf-8')
print(f'Detected {result[`"total_files`"]} files')
"@
```

Replace INPUT_PATH with the actual path. Present a clean summary:

```
Corpus: X files · ~Y words
  code:     N files (.rs .ts .go ...)
  docs:     N files (.md .txt ...)
  papers:   N files (.pdf ...)
  images:   N files
  video:    N files (.mp4 .mp3 ...)
```

Omit any category with 0 files. Then act on it:
- If `total_files` is 0: stop with "No supported files found in [path]."
- If `total_words` > 2,000,000 OR `total_files` > 500: show warning and suggest narrowing.
- Otherwise: proceed to Step 3.

## Step 3 — Extract entities and relationships

This step has two parts: **structural extraction** (AST, deterministic, free) and **semantic extraction** (LLM, costs tokens).

> **graphify needs no API key.** Code is extracted structurally (AST) with no LLM. Semantic extraction (for docs, papers, images) uses Gemini **only if** `GEMINI_API_KEY`/`GOOGLE_API_KEY` is set; otherwise the host agent is the LLM. graphify does NOT read `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`.

### Part A — Structural extraction (AST) for code files

```powershell
$PyInterp = (Get-Content graphify-out/.graphify_python -Raw).Trim()
& $PyInterp -c @"
import sys, json
from graphify.extract import collect_files, extract
from pathlib import Path

detect = json.loads(Path('graphify-out/.graphify_detect.json').read_text(encoding='utf-8'))
code_files = []
for f in detect.get('files', {}).get('code', []):
    code_files.extend(collect_files(Path(f)) if Path(f).is_dir() else [Path(f)])

if code_files:
    result = extract(code_files, cache_root=Path('INPUT_PATH'))
    Path('graphify-out/.graphify_ast.json').write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding='utf-8')
    print(f'AST: {len(result[`"nodes`"])} nodes, {len(result[`"edges`"])} edges')
else:
    Path('graphify-out/.graphify_ast.json').write_text(json.dumps({'nodes':[],'edges':[],'input_tokens':0,'output_tokens':0}, ensure_ascii=False), encoding='utf-8')
    print('No code files - skipping AST extraction')
"@
```

### Part B — Semantic extraction (for docs, papers, images)

**Fast path:** If detection found zero docs, papers, and images (code-only corpus), skip Part B and write an empty semantic file:

```powershell
$PyInterp = (Get-Content graphify-out/.graphify_python -Raw).Trim()
& $PyInterp -c @"
import json
from pathlib import Path
Path('graphify-out/.graphify_semantic.json').write_text(json.dumps({'nodes':[],'edges':[],'hyperedges':[],'input_tokens':0,'output_tokens':0}), encoding='utf-8')
"@
```

For corpora with docs/papers/images, follow the same subagent dispatch pattern as the original skill (dispatch in parallel using subagents with `subagent_type="general-purpose"`).

### Part C — Merge AST + semantic

```powershell
$PyInterp = (Get-Content graphify-out/.graphify_python -Raw).Trim()
& $PyInterp -c @"
import sys, json
from pathlib import Path

ast = json.loads(Path('graphify-out/.graphify_ast.json').read_text(encoding='utf-8'))
sem = json.loads(Path('graphify-out/.graphify_semantic.json').read_text(encoding='utf-8'))

seen = {n['id'] for n in ast['nodes']}
merged_nodes = list(ast['nodes'])
for n in sem['nodes']:
    if n['id'] not in seen:
        merged_nodes.append(n)
        seen.add(n['id'])

merged = {
    'nodes': merged_nodes,
    'edges': ast['edges'] + sem['edges'],
    'hyperedges': sem.get('hyperedges', []),
    'input_tokens': sem.get('input_tokens', 0),
    'output_tokens': sem.get('output_tokens', 0),
}
Path('graphify-out/.graphify_extract.json').write_text(json.dumps(merged, indent=2, ensure_ascii=False), encoding='utf-8')
print(f'Merged: {len(merged_nodes)} nodes, {len(merged[`"edges`"])} edges')
"@
```

## Step 4 — Build graph, cluster, analyze

```powershell
New-Item -ItemType Directory -Path graphify-out -Force | Out-Null
$PyInterp = (Get-Content graphify-out/.graphify_python -Raw).Trim()
& $PyInterp -c @"
import sys, json
from graphify.build import build_from_json
from graphify.cluster import cluster, score_all
from graphify.analyze import god_nodes, surprising_connections, suggest_questions
from graphify.report import generate
from graphify.export import to_json
from pathlib import Path

extraction = json.loads(Path('graphify-out/.graphify_extract.json').read_text(encoding='utf-8'))
detection  = json.loads(Path('graphify-out/.graphify_detect.json').read_text(encoding='utf-8'))

G = build_from_json(extraction, root='INPUT_PATH', directed=IS_DIRECTED)
if G.number_of_nodes() == 0:
    print('ERROR: Graph is empty - extraction produced no nodes.')
    raise SystemExit(1)

communities = cluster(G)
cohesion = score_all(G, communities)
tokens = {'input': extraction.get('input_tokens', 0), 'output': extraction.get('output_tokens', 0)}
gods = god_nodes(G)
surprises = surprising_connections(G, communities)
labels = {cid: 'Community ' + str(cid) for cid in communities}
questions = suggest_questions(G, communities, labels)

wrote = to_json(G, communities, 'graphify-out/graph.json')
if not wrote:
    print('ERROR: refused to shrink graphify-out/graph.json (#479).')
    raise SystemExit(1)

report = generate(G, communities, cohesion, labels, gods, surprises, detection, tokens, 'INPUT_PATH', suggested_questions=questions)
Path('graphify-out/GRAPH_REPORT.md').write_text(report, encoding='utf-8')
analysis = {
    'communities': {str(k): v for k, v in communities.items()},
    'cohesion': {str(k): v for k, v in cohesion.items()},
    'gods': gods,
    'surprises': surprises,
    'questions': questions,
}
Path('graphify-out/.graphify_analysis.json').write_text(json.dumps(analysis, indent=2, ensure_ascii=False), encoding='utf-8')
print(f'Graph: {G.number_of_nodes()} nodes, {G.number_of_edges()} edges, {len(communities)} communities')
"@
```

Replace INPUT_PATH and IS_DIRECTED (True/False) with actual values.

## Step 5 — Label communities

Read `graphify-out/.graphify_analysis.json`. For each community key, examine its node labels and write a 2-5 word plain-language name. Then regenerate the report with labels.

## Step 6 — Generate HTML visualization

```powershell
graphify export html
```

## Step 9 — Clean up and report

```powershell
# Clean up temp files
Remove-Item graphify-out/.graphify_detect.json -Force -ErrorAction SilentlyContinue
Remove-Item graphify-out/.graphify_extract.json -Force -ErrorAction SilentlyContinue
Remove-Item graphify-out/.graphify_ast.json -Force -ErrorAction SilentlyContinue
Remove-Item graphify-out/.graphify_semantic.json -Force -ErrorAction SilentlyContinue
Remove-Item graphify-out/.graphify_analysis.json -Force -ErrorAction SilentlyContinue
Get-ChildItem graphify-out/.graphify_chunk_*.json -ErrorAction SilentlyContinue | Remove-Item -Force
Remove-Item graphify-out/.needs_update -Force -ErrorAction SilentlyContinue
```

Report outputs:
```
Graph complete. Outputs in graphify-out/

  graph.html            - interactive graph, open in browser
  GRAPH_REPORT.md       - audit report
  graph.json            - raw graph data
```

Then paste God Nodes, Surprising Connections, and Suggested Questions from GRAPH_REPORT.md.

---

## For /graphify query (existing graph)

When `graphify-out/graph.json` exists and the user asks a codebase question:

```powershell
graphify query "<question>"
```

If the `graphify` CLI is unavailable, fall back to inline NetworkX traversal of `graphify-out/graph.json`. Answer using only what the graph contains, and quote `source_location` when citing facts.

## For /graphify path

```powershell
graphify path "ConceptA" "ConceptB"
```

## For /graphify explain

```powershell
graphify explain "NodeName"
```

## For --update (incremental)

Re-extracts only new or changed files:

```powershell
graphify . --update
```

## For --cluster-only

Reruns clustering on the existing graph without re-extraction:

```powershell
graphify . --cluster-only
```

---

## Honesty Rules

- Never invent an edge. If unsure, use AMBIGUOUS.
- Never skip the corpus check warning.
- Always show token cost in the report.
- Never hide cohesion scores behind symbols — show the raw number.
- Never run HTML viz on a graph with more than 5,000 nodes without warning the user.

## Graphify Output Structure

```
graphify-out/
├── graph.html           # Interactive graph visualization
├── GRAPH_REPORT.md      # God nodes, surprising connections, suggested questions
├── graph.json           # Persistent knowledge graph (query without re-reading)
├── obsidian/            # Open as Obsidian vault (if --obsidian)
├── wiki/                # Wikipedia-style articles (if --wiki)
└── cache/               # SHA256 cache — re-runs only process changed files
    ├── ast/             # Per-file AST analysis
    ├── semantic/        # Semantic relationship extraction
    └── stat-index.json  # File metadata and hash index
```

## Integration with Codebase Navigation

This skill is the **primary codebase navigation tool** for ClawCrew. Per `.gemini/steering.md` and `AGENTS.md`:

1. **Graph first** — query `graphify-out/` before any grep or directory walk.
2. **Grep second** — only for line-level detail after graphify identifies target files.
3. **Direct file read** — only open files identified by steps 1–2.

If `graphify-out/` does not exist, run `graphify .` to generate the graph before proceeding with any codebase query.

