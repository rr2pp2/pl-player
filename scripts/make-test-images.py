"""テスト用の画像（明るい背景・ジャケット2種）を手元で描く。外部の素材は使わない。
使い方：python scripts/make-test-images.py  →  src-tauri/tests/fixtures/
"""
from PIL import Image, ImageDraw
import os
out = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "tests", "fixtures")
W, H = 1920, 1080
img = Image.new("RGB", (W, H))
d = ImageDraw.Draw(img)
for y in range(H):
    t = y / H
    d.line([(0, y), (W, y)], fill=(int(250 - 60 * t), int(215 - 40 * t), int(170 + 60 * t)))
for i in range(12):
    d.ellipse([i * 170 - 80, 600 - (i % 3) * 120, i * 170 + 220, 900 - (i % 3) * 120], fill=(255, 255, 255))
img.save(os.path.join(out, "bg_bright.png"))
for name, col in [("cover_a.png", (124, 108, 255)), ("cover_b.jpg", (255, 140, 90))]:
    c = Image.new("RGB", (600, 600), col)
    dd = ImageDraw.Draw(c)
    dd.ellipse([150, 150, 450, 450], fill=(255, 255, 255))
    dd.ellipse([270, 270, 330, 330], fill=col)
    c.save(os.path.join(out, name))
print("ok")
