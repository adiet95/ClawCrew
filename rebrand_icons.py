import os
import shutil
from PIL import Image

logo_path = r"C:\Users\adiet\.gemini\antigravity\brain\7c9b6e8b-fc91-41cf-91b5-5c772f2874f2\clawcrew_logo_notext_1790505530947.jpg"
img = Image.open(logo_path).convert("RGBA")

# Ensure transparent background if possible (or just keep it as is, it's white)
# Actually, the user's generated image is JPG, so no alpha channel.
# Let's save a base PNG version.
base_png_path = "base_logo.png"
img.save(base_png_path, "PNG")

# Let's define the paths and target sizes for icons
tauri_icons_dir = os.path.join("apps", "tauri", "icons")
web_public_dir = os.path.join("web", "public")
web_dist_dir = os.path.join("web", "dist")
docs_assets_dir = os.path.join("docs", "assets")

icons_to_replace = [
    # Tauri
    (os.path.join(tauri_icons_dir, "32x32.png"), (32, 32)),
    (os.path.join(tauri_icons_dir, "64x64.png"), (64, 64)),
    (os.path.join(tauri_icons_dir, "128x128.png"), (128, 128)),
    (os.path.join(tauri_icons_dir, "128x128@2x.png"), (256, 256)),
    (os.path.join(tauri_icons_dir, "icon.png"), (512, 512)),
    (os.path.join(tauri_icons_dir, "Square30x30Logo.png"), (30, 30)),
    (os.path.join(tauri_icons_dir, "Square44x44Logo.png"), (44, 44)),
    (os.path.join(tauri_icons_dir, "Square71x71Logo.png"), (71, 71)),
    (os.path.join(tauri_icons_dir, "Square89x89Logo.png"), (89, 89)),
    (os.path.join(tauri_icons_dir, "Square107x107Logo.png"), (107, 107)),
    (os.path.join(tauri_icons_dir, "Square142x142Logo.png"), (142, 142)),
    (os.path.join(tauri_icons_dir, "Square150x150Logo.png"), (150, 150)),
    (os.path.join(tauri_icons_dir, "Square284x284Logo.png"), (284, 284)),
    (os.path.join(tauri_icons_dir, "Square310x310Logo.png"), (310, 310)),
    (os.path.join(tauri_icons_dir, "StoreLogo.png"), (50, 50)),
    
    # Web
    (os.path.join(web_public_dir, "logo.png"), (512, 512)),
    (os.path.join(web_dist_dir, "logo.png"), (512, 512)),
    
    # Docs
    (os.path.join(docs_assets_dir, "zeroclaw-image.png"), (512, 512)),
    (os.path.join(docs_assets_dir, "zeroclaw.png"), (512, 512)),
    (os.path.join(docs_assets_dir, "zeroclaw-banner.png"), (800, 400)), # Resized for banner
]

for path, size in icons_to_replace:
    if os.path.exists(path):
        resized = img.resize(size, Image.Resampling.LANCZOS)
        resized.save(path, "PNG")
        print(f"Replaced {path}")

# For .ico file
ico_path = os.path.join(tauri_icons_dir, "icon.ico")
if os.path.exists(ico_path):
    img.save(ico_path, format="ICO", sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    print(f"Replaced {ico_path}")

# For .icns file, just use the png as icns is macOS and hard to generate from python without specific libraries.
# Usually Tauri falls back to png or ico if we just replace the icon.png. But let's copy the base png to icon.icns as a hack, or just leave it for now.
# Actually, pillow can't save .icns natively. We will leave it or overwrite with png hoping mac understands it.

# GitHub assets
gh_assets = [
    os.path.join(".github", "assets", "zeroclaw-logo.png"),
]
for p in gh_assets:
    if os.path.exists(p):
        resized = img.resize((512, 512), Image.Resampling.LANCZOS)
        resized.save(p, "PNG")

print("Icon rebranding complete!")
