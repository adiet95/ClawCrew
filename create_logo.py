from PIL import Image, ImageDraw

def create_rounded_mask(size, radius):
    mask = Image.new("L", size, 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0, size[0], size[1]), radius=radius, fill=255)
    return mask

def generate_logo():
    bg_color = "#660033"
    size = (1024, 1024)
    
    # Base background
    base = Image.new("RGBA", size, bg_color)
    
    # Load Ferris
    ferris_path = r"C:\Users\adiet\.gemini\antigravity\brain\7c9b6e8b-fc91-41cf-91b5-5c772f2874f2\.user_uploaded\media_1790506042614.png"
    ferris = Image.open(ferris_path).convert("RGBA")
    
    # Bottom Crab (Big)
    scale_bottom = 1.8
    new_w = int(ferris.width * scale_bottom)
    new_h = int(ferris.height * scale_bottom)
    f_bottom = ferris.resize((new_w, new_h), Image.Resampling.LANCZOS)
    base.paste(f_bottom, ((size[0] - new_w) // 2, size[1] - int(new_h * 0.7)), f_bottom)
    
    # Top Left Crab
    scale_top = 1.0
    new_w = int(ferris.width * scale_top)
    new_h = int(ferris.height * scale_top)
    f_tl = ferris.resize((new_w, new_h), Image.Resampling.LANCZOS).rotate(-135, expand=True)
    base.paste(f_tl, (-100, -100), f_tl)
    
    # Top Right Crab
    f_tr = ferris.resize((new_w, new_h), Image.Resampling.LANCZOS).rotate(135, expand=True)
    base.paste(f_tr, (size[0] - f_tr.width + 100, -100), f_tr)
    
    # Apply rounded corners
    mask = create_rounded_mask(size, 200)
    final_logo = Image.new("RGBA", size, (0,0,0,0))
    final_logo.paste(base, (0,0), mask)
    
    # Save base logo
    final_logo.save("base_logo.png", "PNG")
    
    # Generate Banner
    banner_size = (1200, 600)
    banner = Image.new("RGBA", banner_size, bg_color)
    
    f_bottom = ferris.resize((new_w*2, new_h*2), Image.Resampling.LANCZOS)
    banner.paste(f_bottom, ((banner_size[0] - f_bottom.width) // 2, banner_size[1] - int(f_bottom.height * 0.6)), f_bottom)
    
    f_tl_banner = ferris.resize((new_w, new_h), Image.Resampling.LANCZOS).rotate(-135, expand=True)
    banner.paste(f_tl_banner, (-50, -50), f_tl_banner)
    
    f_tr_banner = ferris.resize((new_w, new_h), Image.Resampling.LANCZOS).rotate(135, expand=True)
    banner.paste(f_tr_banner, (banner_size[0] - f_tr_banner.width + 50, -50), f_tr_banner)
    
    banner.save("docs/assets/clawcrew-banner.png", "PNG")

if __name__ == "__main__":
    generate_logo()
    print("Logo and banner generated successfully.")
