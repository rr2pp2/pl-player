"""アプリのアイコンの元画像（1024x1024 PNG）を手元で描く。外部の素材は使わない。
使い方：python scripts/make-icon.py  →  src-tauri/icons/app-icon.png
そのあと：npx tauri icon src-tauri/icons/app-icon.png  （各サイズを手元で生成）
"""
from PIL import Image, ImageDraw, ImageFilter
import os

S = 1024
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)
# 角丸の背景（紫〜紺のグラデーション）
grad = Image.new("RGBA", (S, S))
for y in range(S):
    t = y / (S - 1)
    r = int(0x7C * (1 - t) + 0x1E * t)
    g = int(0x6C * (1 - t) + 0x1B * t)
    b = int(0xFF * (1 - t) + 0x4B * t)
    ImageDraw.Draw(grad).line([(0, y), (S, y)], fill=(r, g, b, 255))
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle([40, 40, S - 40, S - 40], radius=200, fill=255)
img.paste(grad, (0, 0), mask)
# 再生の三角と、2本の音の線
d = ImageDraw.Draw(img)
d.polygon([(380, 300), (380, 724), (720, 512)], fill=(255, 255, 255, 245))
d.rounded_rectangle([250, 330, 300, 694], radius=25, fill=(255, 255, 255, 170))
d.rounded_rectangle([160, 410, 205, 614], radius=22, fill=(255, 255, 255, 120))
out = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons", "app-icon.png")
img.save(out)
print(out)
