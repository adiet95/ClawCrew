import re
import os

# Update roadmap
path_roadmap = 'docs/clawcrew-kirocrew-roadmap.md'
if os.path.exists(path_roadmap):
    with open(path_roadmap, 'r', encoding='utf-8') as f:
        text = f.read()

    text = text.replace('1 architectural item + UI parity remaining', 'UI parity remaining - backend projection incomplete')

    # Remove P0 section
    text = re.sub(r'## Phase 0 / P0: Durable Work and Session Foundation.*?## Phase 2 / P2:', '## Phase 2 / P2:', text, flags=re.DOTALL)
    # Remove P1 if it exists between P0 and P2
    text = re.sub(r'## Phase 1 / P1: Autonomous Workflow Engine.*?## Phase 2 / P2:', '## Phase 2 / P2:', text, flags=re.DOTALL)

    # Remove P3 section
    text = re.sub(r'## P3: Product Maturity.*?## Suggested Delivery Order', '## Suggested Delivery Order', text, flags=re.DOTALL)

    # Remove task references at the bottom
    text = text.replace('- [Phase 0 tasks](clawcrew-phase0.md)\n', '')
    text = text.replace('- [Phase 1 tasks](clawcrew-phase1.md)\n', '')
    text = text.replace('- [Phase 3 tasks](clawcrew-phase3.md)\n', '')

    # Update audit text
    text = text.replace('**route-selection wiring** (move the reliability contract below the runtime),\n', '')

    with open(path_roadmap, 'w', encoding='utf-8') as f:
        f.write(text)

# Update UI parity
path_ui = 'docs/kirocrew-ui-parity.md'
if os.path.exists(path_ui):
    with open(path_ui, 'r', encoding='utf-8') as f:
        text = f.read()
    
    text = text.replace('| **Apps** (install/enable/update/rollback, pages, tools) | - | ? belum ada |', '| **Apps** (install/enable/update/rollback, pages, tools) | `/apps` | ? Placeholder ada, backend belum |')
    text = text.replace('| **Instances / remote crews** | - | ? belum ada |', '| **Instances / remote crews** | `/instances` | ? Placeholder ada, backend belum |')
    text = text.replace('| **Recovery console terpadu** (lost/needs_review lintas entitas) | - | ? belum ada |', '| **Recovery console terpadu** (lost/needs_review lintas entitas) | `/recovery` | ? Placeholder ada, backend belum |')
    
    with open(path_ui, 'w', encoding='utf-8') as f:
        f.write(text)

# Clean up phase2.md (No need to delete Phase 2 entirely as it has some unfinished tasks, but let's update it to ensure it reflects current state)
