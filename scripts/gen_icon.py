#!/usr/bin/env python3
"""Generate RemoteLab app icon — minimal terminal >_ theme."""

from PIL import Image, ImageDraw
import os

SIZE = 1024
OUT_DIR = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")


def draw_icon(size=SIZE):
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    pad = int(size * 0.06)
    r = int(size * 0.22)

    # Dark rounded rect background
    draw.rounded_rectangle(
        (pad, pad, size - pad, size - pad), radius=r, fill=(22, 27, 34, 255)
    )

    # Subtle border
    draw.rounded_rectangle(
        (pad, pad, size - pad, size - pad),
        radius=r,
        outline=(50, 60, 75, 120),
        width=max(2, size // 256),
    )

    # Center the ">_" prompt
    cx = size // 2
    cy = size // 2

    accent = (100, 210, 255, 255)
    lw = int(size * 0.05)

    # ">" — centered vertically, shifted left
    chevron_h = int(size * 0.22)
    chevron_w = int(size * 0.14)
    gt_x = cx - int(size * 0.16)
    gt_y = cy - chevron_h // 2

    pts = [
        (gt_x, gt_y),
        (gt_x + chevron_w, gt_y + chevron_h // 2),
        (gt_x, gt_y + chevron_h),
    ]
    draw.line(pts, fill=accent, width=lw, joint="curve")

    # "_" — underscore / cursor block to the right of ">"
    cursor_x = cx + int(size * 0.04)
    cursor_y = cy + int(size * 0.06)
    cursor_w = int(size * 0.16)
    cursor_h = int(size * 0.05)
    draw.rounded_rectangle(
        (cursor_x, cursor_y, cursor_x + cursor_w, cursor_y + cursor_h),
        radius=cursor_h // 4,
        fill=accent,
    )

    return img


def main():
    os.makedirs(OUT_DIR, exist_ok=True)

    icon = draw_icon(1024)

    # Save all required sizes
    sizes = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "tray-icon.png": 32,
    }
    for name, sz in sizes.items():
        resized = icon.resize((sz, sz), Image.LANCZOS)
        resized.save(os.path.join(OUT_DIR, name))
        print(f"  Saved {name} ({sz}x{sz})")

    # icon.ico — Windows
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    ico_images = [icon.resize((s, s), Image.LANCZOS) for s in ico_sizes]
    ico_images[0].save(
        os.path.join(OUT_DIR, "icon.ico"),
        format="ICO",
        sizes=[(s, s) for s in ico_sizes],
        append_images=ico_images[1:],
    )
    print("  Saved icon.ico")

    # icon.icns — macOS
    import subprocess
    import tempfile

    iconset_dir = os.path.join(tempfile.gettempdir(), "RemoteLab.iconset")
    os.makedirs(iconset_dir, exist_ok=True)

    icns_sizes = [16, 32, 64, 128, 256, 512, 1024]
    for s in icns_sizes:
        resized = icon.resize((s, s), Image.LANCZOS)
        if s <= 512:
            resized.save(os.path.join(iconset_dir, f"icon_{s}x{s}.png"))
        if s >= 32:
            half = s // 2
            resized.save(os.path.join(iconset_dir, f"icon_{half}x{half}@2x.png"))

    subprocess.run(
        ["iconutil", "-c", "icns", iconset_dir, "-o", os.path.join(OUT_DIR, "icon.icns")],
        check=True,
    )
    print("  Saved icon.icns")

    import shutil
    shutil.rmtree(iconset_dir, ignore_errors=True)

    # Preview
    preview = icon.resize((128, 128), Image.LANCZOS)
    preview.save(os.path.join(OUT_DIR, "preview.png"))
    print("\nDone!")


if __name__ == "__main__":
    main()
