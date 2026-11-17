#!/usr/bin/env python3
"""Generates every icon and in-app logo from the official logo, branding/logo.png (1024 px,
transparent outside the rounded square). Run it again after changing the logo and commit the results.

  - Android (app + verifier), android/branding/res/: adaptive icon foreground, in-app logo,
    notification icon (white silhouette)
  - iOS, apple/ios/App/Assets.xcassets/: AppIcon (full bleed, opaque: iOS applies its own mask) and
    the Logo image (launch screen, in-app)
  - Desktop: Tauri icon.png 512 + multi-size icon.ico, UI logo/favicon, NSIS installer bitmaps

Usage: python scripts/make_icons.py      (needs Pillow)
"""
import pathlib

from PIL import Image, ImageStat

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = Image.open(ROOT / "branding/logo.png").convert("RGBA")
assert SRC.size == (1024, 1024), "branding/logo.png must be 1024 x 1024"

SQUARE = (103, 103, 921, 921)   # the rounded dark square inside the 1024 canvas
GLYPH = (290, 270, 740, 750)    # the "P" and the dot, with margin; plain background around it


def mean(box):
    return tuple(round(c) for c in ImageStat.Stat(SRC.crop(box).convert("RGB")).mean)


BG = mean((200, 200, 260, 260))   # dark background (Android adaptive background: same value)
DOT = mean((560, 420, 580, 440))  # teal dot
WHITE = (255, 255, 255)


def glyph_layer():
    """The P and the dot on transparent: un-mixes each pixel as BG + a*(WHITE-BG) + b*(DOT-BG)."""
    d1 = [w - g for w, g in zip(WHITE, BG)]
    d2 = [t - g for t, g in zip(DOT, BG)]
    m11, m12, m22 = (sum(x * y for x, y in zip(u, v)) for u, v in ((d1, d1), (d1, d2), (d2, d2)))
    det = m11 * m22 - m12 * m12
    out = Image.new("RGBA", SRC.size, (0, 0, 0, 0))
    src, dst = SRC.load(), out.load()
    for y in range(GLYPH[1], GLYPH[3]):
        for x in range(GLYPH[0], GLYPH[2]):
            v = [c - g for c, g in zip(src[x, y][:3], BG)]
            r1 = sum(p * q for p, q in zip(d1, v))
            r2 = sum(p * q for p, q in zip(d2, v))
            a = max(0.0, (m22 * r1 - m12 * r2) / det)
            b = max(0.0, (m11 * r2 - m12 * r1) / det)
            s = a + b
            if s < 0.02:
                continue
            color = tuple(round((a * w + b * t) / s) for w, t in zip(WHITE, DOT))
            dst[x, y] = color + (round(min(s, 1.0) * 255),)
    return out


glyph = glyph_layer()

# Check: the glyph over the background must rebuild the original (within anti-aliasing noise).
rebuilt = Image.new("RGB", SRC.size, BG)
rebuilt.paste(glyph, mask=glyph)
a, b = rebuilt.crop(GLYPH).tobytes(), SRC.crop(GLYPH).convert("RGB").tobytes()
err = sum(abs(p - q) for p, q in zip(a, b)) / len(a)
assert err < 2, f"glyph extraction drifted from the logo (mean error {err:.2f})"

def save(img, rel, **kw):
    path = ROOT / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    img.save(path, optimize=True, **kw)  # formats without "optimize" ignore it


def fit(img, box, pad):
    """img scaled to fit a box x box canvas with pad px on every side, centered, transparent."""
    img = img.crop(img.getbbox())
    k = (box - 2 * pad) / max(img.size)
    img = img.resize((round(img.width * k), round(img.height * k)), Image.LANCZOS)
    out = Image.new("RGBA", (box, box), (0, 0, 0, 0))
    out.alpha_composite(img, ((box - img.width) // 2, (box - img.height) // 2))
    return out


LOGO = SRC.crop((96, 96, 928, 928))  # the logo as is (rounded, transparent corners), thin margin


def px(size):
    return LOGO.resize((size, size), Image.LANCZOS)



# iOS: one 1024 universal icon, opaque, full bleed; Logo at 96 pt for the launch screen and the UI.
save(rebuilt.crop(SQUARE).resize((1024, 1024), Image.LANCZOS),
     "apple/ios/App/Assets.xcassets/AppIcon.appiconset/AppIcon.png")
for scale in (1, 2, 3):
    save(px(96 * scale), f"apple/ios/App/Assets.xcassets/Logo.imageset/Logo@{scale}x.png")

# Android: 108 dp layer at xxxhdpi (432 px); the logo square fills the 72 dp visible area.
fg = Image.new("RGBA", (432, 432), (0, 0, 0, 0))
fg.alpha_composite(glyph.crop(SQUARE).resize((288, 288), Image.LANCZOS), (72, 72))
save(fg, "android/branding/res/drawable-nodpi/ic_launcher_foreground.png")
save(px(288), "android/branding/res/drawable-nodpi/logo.png")  # 72 dp at xxxhdpi
# Notification icon: 24 dp (96 px at xxxhdpi), white silhouette, 2 dp margin.
white = Image.new("RGBA", glyph.size, (255, 255, 255, 0))
white.putalpha(glyph.getchannel("A"))
save(fit(white, 96, 8), "android/branding/res/drawable-xxxhdpi/ic_stat_logo.png")

# Desktop: app icons, UI logo + favicon, NSIS installer bitmaps (24-bit BMP, no alpha).
save(px(512), "desktop/app/src-tauri/icons/icon.png")
save(px(256), "desktop/app/src-tauri/icons/icon.ico", sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
save(px(256), "desktop/app/ui/public/logo.png")
header = Image.new("RGB", (150, 57), WHITE)
header.paste(px(45), (99, 6), px(45))
save(header, "desktop/app/src-tauri/windows/installer-header.bmp")
sidebar, mark = Image.new("RGB", (164, 314), BG), fit(glyph, 164, 34)
sidebar.paste(mark, (0, 60), mark)
save(sidebar, "desktop/app/src-tauri/windows/installer-sidebar.bmp")

print(f"background #{''.join(f'{c:02X}' for c in BG)}, dot #{''.join(f'{c:02X}' for c in DOT)}, "
      f"extraction error {err:.2f}")
