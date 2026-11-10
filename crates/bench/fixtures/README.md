# Benchmark image fixtures

Deterministic 800×600 images used by the `image/decode-*` rows of `crates/bench`.

- `jpeg-800x600.jpg`: baseline JPEG, quality 85, subsampling 4:2:0. Generated from a
  deterministic RGB gradient with circles/lines (Pillow 13, no randomness).
- `gif-800x600.gif`: single-frame indexed GIF (256-colour adaptive palette) from the
  same source image.
- PNG decoding uses the committed `crates/image/tests/fixtures/png/large_800x600.png`
  (800×600, 13 947 B) so the row exercises the crate's own PNG fixture.

Regenerate the JPEG/GIF with:

```python
from PIL import Image, ImageDraw

w, h = 800, 600
img = Image.new("RGB", (w, h))
px = img.load()
for y in range(h):
    for x in range(w):
        px[x, y] = (int(30 + 180 * x / (w - 1)), int(40 + 160 * y / (h - 1)),
                    int(90 + 120 * ((x + y) % 256) / 255))
d = ImageDraw.Draw(img)
for i in range(36):
    cx, cy, rad = (i * 97) % w, (i * 71) % h, 18 + (i * 13) % 70
    d.ellipse([cx - rad, cy - rad, cx + rad, cy + rad], width=3,
              outline=((i * 40) % 256, (255 - i * 30) % 256, (i * 70) % 256))
for i in range(24):
    d.line([(i * 37 % w, i * 53 % h), ((i * 91 + 150) % w, (i * 67 + 80) % h)],
           width=2, fill=((255 - i * 9) % 256, i * 10 % 256, (i * 20) % 256))
img.save("jpeg-800x600.jpg", "JPEG", quality=85, subsampling=2, optimize=True)
img.convert("P", palette=Image.Palette.ADAPTIVE, colors=256).save("gif-800x600.gif", "GIF", optimize=True)
```
