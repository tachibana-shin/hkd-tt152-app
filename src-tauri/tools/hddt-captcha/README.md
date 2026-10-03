# HDDT captcha reference-data pipeline

Offline tooling that generates `templates.bin` + `parity.txt` for the Rust solver
`packages/hddt-captcha/src/solver.rs::GlyphTemplateSolver`.

## Why this works

The `hoadondientu.gdt.gov.vn` captcha is 6 SVG glyphs rendered with the
**portal's own font** (not matching any system font), but it is the **same font
every time**, only position/scale differ. So collecting labeled samples → raster
each glyph to a 48×48 bitmap → IoU matching solves it **fully offline**, with no
paid service required.

The charset has **30 characters**: `23456789ABCDEFGHJKMNPQRSTVWXYZ` (the server
excludes `0 1 I L O U` — the confusable ones). `P` is rare (1 glyph in the first
222) but real: 8 portal-accepted samples contain it, so it is not excluded.

Measured on the live portal (25/09/2026): leave-one-out 1356/1356, and
**100/100 consecutive captchas solved** (automatic logins all accepted).

## Components

| File | Role |
|---|---|
| `glyph_algo.py` | **Source of truth** of the rasterizer (paths M/L/Q/Z, nonzero fill, normalize to 40px + pad 1, center in 48×48). The Rust implementation is a 1-1 port of this file. |
| `export_templates.py` | Generates `templates.bin` + `parity.txt` from `labeled/`. Writes straight into `packages/hddt-captcha/src/`. |
| `evaluate.py` | Re-classifies the labeled set through `templates.bin` + leave-one-out. Expects 100%. |
| `collect_captchas.py` | Fetches fresh captchas from the portal (urllib, no browser) to grow the sample set. |
| `hard/` | Captchas the solver got wrong; read them by hand to label (captcha keys expire in minutes, so these are for labeling only). |
| `labeled/` | Labeled samples `<index>_<key>_<ANSWER>.svg` (226 files, 1356 glyphs) — commit cả lại để dựng lại `templates.bin` bất cứ lúc nào. |

## Data format

- `templates.bin`: repeated `[label 1 byte][bitmap 288 bytes]` — bitmap 48×48 =
  2304 bits, row-major, MSB-first. Currently 1356 entries (226 mẫu đã gán nhãn).
- `parity.txt`: one line `label<TAB>path<TAB>hex(288 bytes)`. The Rust unit test
  `parity_with_python_reference` re-renders each path and compares against the
  hex → catches any divergence between the Python and Rust implementations.

## Workflow

Only `python3` + `numpy` are required.

```bash
cd src-tauri/tools/hddt-captcha

# 1) (optional) fetch fresh captchas → new/
python3 collect_captchas.py 6

# 2) Open the images in new/, read the captcha, rename to
#    <index>_<key>_<ANSWER>.svg and move into labeled/  (unlabeled files
#    in labeled/ are skipped by the exporter)

# 3) Regenerate the reference data
python3 export_templates.py

# 4) Verify
python3 evaluate.py
cd ../../ && cargo test --lib hddt
```

### Adding new samples to increase coverage

1. `python3 collect_captchas.py <n>` → `new/`.
2. Open each SVG in a browser and read its 6 characters (note: only characters
   from the 29-character charset above can appear), then rename
   `new/<index>_<key>.svg` → `labeled/<index>_<key>_<ANSWER>.svg` (exactly 6
   characters).
3. Re-run `export_templates.py` → `evaluate.py` → `cargo test`.

## Notes

- The captcha paths only use `M/L/Q/Z`; if the portal ever changes the format
  (adds another command), update **both** `glyph_algo.py` and `glyph.rs`, then
  re-run the parity test.
- `new/` and `__pycache__/` are not committed.
- This is reference data and contains **no login credentials**.


## Growing the set without reading images

Captcha keys expire within minutes, so a sample cannot be labeled and verified
later. Two ways to add verified data:

```bash
cd src-tauri
# Tự thu hoạch: lấy captcha → solver đoán → đăng nhập thật.
# Cổng chấp nhận = chuỗi đoán đúng 100% → lưu thẳng vào labeled/.
HARVEST_ROUNDS=60 cargo test --lib live_harvest_captcha_templates -- --ignored
# Cổng từ chối → biến thể glyph chưa có template, lưu vào `hard/` để đọc tay.
```

Sau khi có mẫu mới (tự thu hoạch hoặc đọc tay trong `hard/`):

```bash
cd src-tauri/tools/hddt-captcha
python3 export_templates.py   # sinh lại templates.bin + parity.txt
python3 evaluate.py           # kiểm tra: classify + leave-one-out
cd ../.. && cargo test --lib hddt   # parity + fixture phải xanh
```

Tên file trong `labeled/` phải đúng dạng `<index>_<key>_<ANSWER>.svg` (3 phần);
`export_templates.py` lấy nhãn từ phần thứ 3 và bỏ qua file sai định dạng.
