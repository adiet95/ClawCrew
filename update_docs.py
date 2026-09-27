import re

# Update gap analysis
with open('docs/clawcrew-kirocrew-gap-analysis.md', 'r', encoding='utf-8') as f:
    gap = f.read()
gap = gap.replace('**`select_route` belum di dispatch live**', '~setara')
gap = gap.replace('1. **Route selection live (P1.5)** - `select_route`/`ProviderHealthRecord` ada\n   di `clawcrew-runtime`, tetapi dispatch provider live (di `clawcrew-providers`)\n   belum membacanya (arah dependency runtime providers). Solusi: pindahkan\n   kontrak reliability ke crate bawah (`clawcrew-api`/`clawcrew-config`) dan\n   teruskan health ke `ReliableModelProvider` saat membangun kandidat.\n   *Effort: L, lintas-crate, terisolasi.*\n\n', '')
if '## Kelebihan ClawCrew' not in gap:
    gap = gap.replace('## Gap yang masih relevan (prioritas)', '## Kelebihan ClawCrew\n\n1. **9router local provider** - dukungan built-in untuk local LLM router.\n2. **Desktop service lifecycle** - toggle service daemon yang dikelola dari Tauri.\n3. **Browser support** - kapabilitas browser natif terintegrasi.\n\n## Gap yang masih relevan (prioritas)')
with open('docs/clawcrew-kirocrew-gap-analysis.md', 'w', encoding='utf-8') as f:
    f.write(gap)

# Update phase1
with open('docs/clawcrew-phase1.md', 'r', encoding='utf-8') as f:
    p1 = f.read()
p1 = p1.replace('Status: **Complete (1 item arsitektural tersisa)**', 'Status: **Complete**')
p1 = re.sub(r'## Sisa\n\n- \*\*P1\.5 - route selection live\*\*.*?Effort: L, lintas-crate, terisolasi\.\*\n', '', p1, flags=re.DOTALL)
with open('docs/clawcrew-phase1.md', 'w', encoding='utf-8') as f:
    f.write(p1)

# Update analys
with open('docs/clawcrew-analys.md', 'r', encoding='utf-8') as f:
    an = f.read()
an = an.replace('Satu item arsitektural masih terbuka: **`select_route` belum dikonsultasikan\ndispatch live** (lihat bagian "Belum ada vs Kiro Crew" #1).\n\n', '')
an = re.sub(r'1\. \*\*Route selection live\*\*.*?membacanya\.\n', '', an, flags=re.DOTALL)
an = an.replace('| Provider runtime | ~85% (route selection live kurang) |', '| Provider runtime | 100% |')
if '## Yang lebih baik di ClawCrew' not in an:
    an = an.replace('## Belum ada / belum penuh vs Kiro Crew', '## Yang lebih baik di ClawCrew\n\n- **9Router local provider** terpasang secara native.\n- **Browser integration**.\n- **Desktop lifecycle** (Tauri toggle service).\n\n## Belum ada / belum penuh vs Kiro Crew')
with open('docs/clawcrew-analys.md', 'w', encoding='utf-8') as f:
    f.write(an)
