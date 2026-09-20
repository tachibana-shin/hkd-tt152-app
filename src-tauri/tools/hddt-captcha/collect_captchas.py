#!/usr/bin/env python3
"""Fetch fresh captchas from the HDDT portal to grow the labeled set.

No browser needed (plain urllib) — the captcha endpoint requires no login. By
default saves to `new/` as `<index>_<key>.svg`. After OPENING the image and
reading the captcha, rename it to `<index>_<key>_<ANSWER>.svg` and move it into
`labeled/`.

    python3 collect_captchas.py [count] [outdir]

Example: python3 collect_captchas.py 6          # fetch 6 captchas → new/
"""
import json
import os
import sys
import time
import urllib.request

URL = "https://hoadondientu.gdt.gov.vn/api/captcha"
UA = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36"
)
HERE = os.path.dirname(os.path.abspath(__file__))


def fetch():
    req = urllib.request.Request(
        URL,
        headers={
            "User-Agent": UA,
            "Accept": "application/json, text/plain, */*",
            "Accept-Language": "vi",
            "Referer": "https://hoadondientu.gdt.gov.vn/",
        },
    )
    with urllib.request.urlopen(req, timeout=20) as r:
        return json.load(r)


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 6
    outdir = sys.argv[2] if len(sys.argv) > 2 else os.path.join(HERE, "new")
    os.makedirs(outdir, exist_ok=True)
    for i in range(1, n + 1):
        try:
            cap = fetch()
            path = os.path.join(outdir, f"{i}_{cap['key']}.svg")
            with open(path, "w") as fh:
                fh.write(cap["content"])
            print(f"#{i} → {os.path.relpath(path)}")
        except Exception as e:  # noqa: BLE001
            print(f"#{i} error: {e}")
        if i < n:
            time.sleep(1.1)
    print(
        "\nRead the captcha in each file above, rename to "
        "<index>_<key>_<ANSWER>.svg and move into labeled/, then run "
        "export_templates.py."
    )


if __name__ == "__main__":
    main()
