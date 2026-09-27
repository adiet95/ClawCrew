import os
import subprocess
import shutil

# Get list of git tracked files
result = subprocess.run(['git', 'ls-files'], stdout=subprocess.PIPE, text=True)
files = result.stdout.strip().split('\n')

def replace_content(content):
    c = content
    c = c.replace('ZeroClaw', 'ClawCrew')
    c = c.replace('zeroclaw', 'clawcrew')
    c = c.replace('ZEROCLAW', 'CLAWCREW')
    c = c.replace('zeroClaw', 'clawCrew')
    return c

# 1. Replace content in all text files
for f in files:
    if not os.path.isfile(f):
        continue
    # Skip binary files/images
    if f.endswith(('.png', '.ico', '.svg', '.icns', '.jpg', '.jpeg', '.gif', '.ttf', '.woff', '.woff2')):
        continue
    
    try:
        with open(f, 'r', encoding='utf-8') as file:
            content = file.read()
    except UnicodeDecodeError:
        continue # skip binaries
        
    new_content = replace_content(content)
    if new_content != content:
        with open(f, 'w', encoding='utf-8') as file:
            file.write(new_content)

# 2. Rename directories (bottom-up to avoid path invalidation)
dirs_to_rename = []
for root, dirs, _ in os.walk('.', topdown=False):
    if '.git' in root or 'node_modules' in root or 'target' in root:
        continue
    for d in dirs:
        if 'zeroclaw' in d.lower():
            dirs_to_rename.append(os.path.join(root, d))

for d in dirs_to_rename:
    parent = os.path.dirname(d)
    base = os.path.basename(d)
    new_base = replace_content(base)
    new_path = os.path.join(parent, new_base)
    os.rename(d, new_path)

# 3. Rename files (bottom-up)
files_to_rename = []
for root, _, files in os.walk('.', topdown=False):
    if '.git' in root or 'node_modules' in root or 'target' in root:
        continue
    for f in files:
        if 'zeroclaw' in f.lower():
            files_to_rename.append(os.path.join(root, f))

for f in files_to_rename:
    parent = os.path.dirname(f)
    base = os.path.basename(f)
    new_base = replace_content(base)
    new_path = os.path.join(parent, new_base)
    os.rename(f, new_path)

print("Rebranding text and filenames complete!")
