"""Print metric-matched fallback @font-face rules for the Atkinson Hyperlegible fonts.

next/font has no precomputed metrics for them (Turbopack warns and skips the fallback), so
app/globals.css carries these rules and app/layout.tsx names them as each font's `fallback`.
Same method as next/font/local: size-adjust from the a-z average advance against the fallback,
then hhea ascent, descent and line gap divided by it. Run from website/: python3 design/font-fallbacks.py
(needs fonttools; fetches the current TrueType files from Google Fonts).
"""

import io
import re
import urllib.request

from fontTools.ttLib import TTFont

# Average a-z advance per em of the local fallbacks (Arial: next/font's value; Courier New: monospace).
FALLBACKS = {"Arial": 934.5116279069767 / 2048, "Courier New": 1229 / 2048}
FONTS = [
    ("Atkinson Hyperlegible Next", "Sakura Body Fallback", "Arial"),
    ("Atkinson Hyperlegible Mono", "Sakura Code Fallback", "Courier New"),
]
SAMPLE = "aaabcdeeeefghiijklmnnoopqrrssttuvwxyz      "


def fetch(family: str) -> TTFont:
    css_url = f"https://fonts.googleapis.com/css2?family={family.replace(' ', '+')}:wght@400"
    css = urllib.request.urlopen(css_url).read().decode()
    ttf = re.search(r"url\((\S+?\.ttf)\)", css).group(1)
    return TTFont(io.BytesIO(urllib.request.urlopen(ttf).read()))


def pct(value: float) -> str:
    return f"{abs(value) * 100:.2f}%"


for family, name, local in FONTS:
    font = fetch(family)
    upm, hhea, cmap, hmtx = font["head"].unitsPerEm, font["hhea"], font.getBestCmap(), font["hmtx"]
    avg = sum(hmtx[cmap[ord(c)]][0] for c in SAMPLE) / len(SAMPLE)
    size = avg / upm / FALLBACKS[local]
    print(f"""/* {family} over {local} */
@font-face {{
  font-family: "{name}";
  src: local("{local}");
  size-adjust: {pct(size)};
  ascent-override: {pct(hhea.ascent / (upm * size))};
  descent-override: {pct(hhea.descent / (upm * size))};
  line-gap-override: {pct(hhea.lineGap / (upm * size))};
}}""")
