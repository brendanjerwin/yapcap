#!/usr/bin/env python3
"""Crop full-popup provider screenshots into the "zoom" style.

Source files: ~/Pictures/Screenshots/{codex,claude,cursor,gemini,copilot}.png
Each source is the full YapCap popup (419x671), captured against a transparent
background. The zoom crop removes the YapCap title bar, provider nav row, and
the Quit/Settings footer so only the provider detail body remains.

Output: resources/screenshots/screenshot-zoom-<provider>.png
"""
import subprocess
from pathlib import Path

SRC_DIR = Path.home() / "Pictures/Screenshots"
DST_DIR = Path(__file__).parent.parent / "resources/screenshots"

PROVIDERS = ["codex", "claude", "cursor", "gemini", "copilot"]

# Source dimensions: 419x671 full popup.
# Top crop removes title bar (40px) + provider nav row (115px) ~= 155px.
# Bottom crop removes Quit/Settings footer (~66px), leaving content end at 605.
CROP_TOP = 155
CROP_BOTTOM = 605
CROP_HEIGHT = CROP_BOTTOM - CROP_TOP
CROP_WIDTH = 419


def main() -> None:
    DST_DIR.mkdir(parents=True, exist_ok=True)
    for provider in PROVIDERS:
        src = SRC_DIR / f"{provider}.png"
        dst = DST_DIR / f"screenshot-zoom-{provider}.png"
        if not src.exists():
            print(f"skip {provider}: missing {src}")
            continue
        crop = f"{CROP_WIDTH}x{CROP_HEIGHT}+0+{CROP_TOP}"
        subprocess.run(
            ["convert", str(src), "-crop", crop, "+repage", str(dst)],
            check=True,
        )
        print(f"Saved {dst}")


if __name__ == "__main__":
    main()
