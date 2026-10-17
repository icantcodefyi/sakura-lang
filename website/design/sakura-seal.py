"""Sakura Seal: builds website/public/brand/* from one parametric construction.

    pip install fonttools uharfbuzz   # only the lockup needs them (shaping applies the font's kerning)
    python3 design/sakura-seal.py      # run from website/; also writes app/icon.svg and app/apple-icon.png
                                       # (the PNG needs rsvg-convert, from librsvg)

Each petal owns a 72 deg wedge. Its sides are the wedge edges moved inward by g/2, so neighbouring
petals are separated by a true cut-out of width g; the base is an arc of radius core + g, concentric
with the core, so the ring gap is also g. The outer silhouette is three cubics (shoulder, lobe,
notch) mirrored about the petal axis. The flower is then centred on its ink box (optical centre).
See design/BRAND.md.
"""
import math, os, re, shutil, subprocess, urllib.parse, urllib.request

A = math.radians(36)  # half of the 72 deg wedge each petal owns

def cubic(p0, p1, p2, p3, n=24):
    out = []
    for i in range(1, n + 1):
        t = i / n; u = 1 - t
        out.append((u**3*p0[0] + 3*u*u*t*p1[0] + 3*u*t*t*p2[0] + t**3*p3[0],
                    u**3*p0[1] + 3*u*u*t*p1[1] + 3*u*t*t*p2[1] + t**3*p3[1]))
    return out

def petal(g, rc, yj, w, yw, xt, yt, yn, k1, k2, k3, nx, ny):
    """Right half in local coords (x right, y out along the petal axis), as bezier commands."""
    h = g / 2
    rb = rc + g
    d = (math.sin(A), math.cos(A))
    n_in = (-math.cos(A), math.sin(A))
    t0 = math.sqrt(rb * rb - h * h)
    B = (t0 * d[0] + h * n_in[0], t0 * d[1] + h * n_in[1])          # base corner on the offset line
    tj = (yj - h * n_in[1]) / d[1]
    J = (tj * d[0] + h * n_in[0], yj)                                # where the side leaves the line
    W = (w, yw)
    T = (xt, yt)
    N = (0.0, yn)
    lj = (yw - yj) * 0.55
    segs = [
        ("L", [J]),
        ("C", [(J[0] + d[0] * lj, J[1] + d[1] * lj), (W[0], W[1] - k1 * 0.6), W]),
        ("C", [(W[0], W[1] + k1), (T[0] + k2, T[1]), T]),
        ("C", [(T[0] - k3, T[1]), (N[0] + nx, N[1] + ny), N]),
    ]
    return B, rb, segs

def mirror(p): return (-p[0], p[1])

def rot(p, deg, c):
    a = math.radians(deg)
    x, y = p
    # local y points out along the axis; screen y grows down
    sx, sy = x, -y
    return (c[0] + sx * math.cos(a) - sy * math.sin(a), c[1] + sx * math.sin(a) + sy * math.cos(a))

def f(v):
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s

def build(P, c=(50, 50)):
    B, rb, segs = petal(**P)
    paths = []
    pts_all = []
    for k in range(5):
        deg = k * 72
        R = lambda p: rot(p, deg, c)
        cmds = []
        # start at the right base corner, go out the right side to the notch, back down the left side
        cmds.append(f"M{f(R(B)[0])} {f(R(B)[1])}")
        for kind, ps in segs:
            cmds.append(kind + " ".join(f"{f(R(p)[0])} {f(R(p)[1])}" for p in ps))
        # left half: reverse of the right half mirrored
        rev = []
        pts = [B] + [ps[-1] for _, ps in segs]
        for i in range(len(segs) - 1, -1, -1):
            kind, ps = segs[i]
            start = pts[i]
            if kind == "L":
                rev.append(("L", [mirror(start)]))
            else:
                rev.append(("C", [mirror(ps[1]), mirror(ps[0]), mirror(start)]))
        for kind, ps in rev:
            cmds.append(kind + " ".join(f"{f(R(p)[0])} {f(R(p)[1])}" for p in ps))
        # close along the base arc (concentric with the core)
        cmds.append(f"A{f(rb)} {f(rb)} 0 0 1 {f(R(B)[0])} {f(R(B)[1])}Z")
        paths.append("".join(cmds))
        # sample outline for bbox
        cur = B
        for kind, ps in segs:
            samp = [ps[0]] if kind == "L" else cubic(cur, *ps)
            for p in samp:
                pts_all += [R(p), R(mirror(p))]
            cur = ps[-1]
    xs = [p[0] for p in pts_all]; ys = [p[1] for p in pts_all]
    return paths, (min(xs), min(ys), max(xs), max(ys))

# Petal shape per optical cut: g gap, rc core radius, J/W/T/N side, widest, lobe and notch points.
MAIN = dict(g=4.2, rc=7.4, yj=16.5, w=17.4, yw=33.5, xt=9.8, yt=47, yn=40.8, k1=8, k2=4.2, k3=2.4, nx=0.9, ny=2.2)
SMALL = dict(g=6.6, rc=9.2, yj=18.5, w=17.8, yw=33.5, xt=10, yt=47, yn=39.4, k1=8, k2=4.2, k3=2.4, nx=1.1, ny=2.8)
SAKURA, MAGENTA, INK, PAPER = "#F386A1", "#D45BB6", "#1E1E1E", "#FEFEFE"
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = os.path.join(ROOT, "public", "brand")
NAME = "Sakura Lang"

def geom(P):
    _, bb = build(P)
    dx = 50 - (bb[0] + bb[2]) / 2; dy = 50 - (bb[1] + bb[3]) / 2
    paths, bb = build(P, c=(50 + dx, 50 + dy))
    return paths, (50 + dx, 50 + dy), P["rc"], bb

def circle_path(c, r):
    return f"M{f(c[0]-r)} {f(c[1])}a{f(r)} {f(r)} 0 1 0 {f(2*r)} 0a{f(r)} {f(r)} 0 1 0 {f(-2*r)} 0Z"

def mark_svg(P, colour=True, label=NAME):
    paths, c, rc, _ = geom(P)
    head = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" role="img" aria-label="{label}">'
    if colour:
        return head + f'<path fill="{SAKURA}" d="{"".join(paths)}"/><circle cx="{f(c[0])}" cy="{f(c[1])}" r="{f(rc)}" fill="{MAGENTA}"/></svg>\n'
    return head + f'<path fill="currentColor" d="{"".join(paths)}{circle_path(c, rc)}"/></svg>\n'

def write(name, s):
    open(os.path.join(OUT, name), "w").write(s)

# --- wordmark: Funnel Display 600, outlines, shaped with HarfBuzz (font kerning), tracked -0.02em
def fetch_font():
    q = urllib.parse.quote(NAME)
    css = urllib.request.urlopen(f"https://fonts.googleapis.com/css2?family=Funnel+Display:wght@600&text={q}").read().decode()
    path = os.path.join("/tmp", "funnel-display-600-sakura-lang.ttf")
    urllib.request.urlretrieve(re.search(r"url\((.+?)\)", css).group(1), path)
    return path

def wordmark(text=NAME, cap=56.0, track=-0.02):
    """Outlined text with its baseline at y=0, scaled so capitals are `cap` units tall."""
    import uharfbuzz as hb
    from fontTools.ttLib import TTFont
    from fontTools.pens.svgPathPen import SVGPathPen
    from fontTools.pens.transformPen import TransformPen
    from fontTools.pens.boundsPen import BoundsPen
    path = fetch_font()
    font = TTFont(path); gs = font.getGlyphSet(); order = font.getGlyphOrder()
    upm = font["head"].unitsPerEm
    s = cap / font["OS/2"].sCapHeight
    hbf = hb.Font(hb.Face(open(path, "rb").read()))
    buf = hb.Buffer(); buf.add_str(text); buf.guess_segment_properties()
    hb.shape(hbf, buf, {"kern": True, "liga": True})
    x = 0.0; d = []; minx = maxx = None; top = bot = 0.0
    infos, poss = buf.glyph_infos, buf.glyph_positions
    for k, (info, pos) in enumerate(zip(infos, poss)):
        name = order[info.codepoint]
        pen = SVGPathPen(gs, ntos=lambda v: f(v))
        gs[name].draw(TransformPen(pen, (s, 0, 0, -s, (x + pos.x_offset) * s, 0)))
        d.append(pen.getCommands())
        bp = BoundsPen(gs); gs[name].draw(bp)
        if bp.bounds:
            lo, hi = (x + bp.bounds[0]) * s, (x + bp.bounds[2]) * s
            minx = lo if minx is None else min(minx, lo); maxx = hi if maxx is None else max(maxx, hi)
            top = min(top, -bp.bounds[3] * s); bot = max(bot, -bp.bounds[1] * s)
        x += pos.x_advance + (track * upm if k < len(infos) - 1 else 0)
    return "".join(d), minx, maxx, top, bot

def lockup():
    cap = 56.0
    wd, lo, hi, ttop, tbot = wordmark(cap=cap)
    paths, c, rc, bb = geom(MAIN)
    mh = bb[3] - bb[1]               # mark ink height (viewBox units)
    scale = (cap * 1.18) / mh        # mark a little taller than the capitals
    gap = cap * 0.3
    mw = (bb[2] - bb[0]) * scale
    # mark centred on the cap-height band; baseline at y=0, band is [-cap, 0]
    mx = -bb[0] * scale; my = -cap / 2 - (bb[1] + mh / 2) * scale
    tx = mw + gap - lo
    W = tx + hi; top = min(my + bb[1] * scale, ttop); bot = max(my + bb[3] * scale, tbot)
    vb = f"0 {f(top)} {f(W)} {f(bot - top)}"
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" role="img" aria-label="{NAME}">'
            f'<style>.w{{fill:{INK}}}@media (prefers-color-scheme:dark){{.w{{fill:{PAPER}}}}}</style>'
            f'<g transform="translate({f(mx)} {f(my)}) scale({scale:.4f})"><path fill="{SAKURA}" d="{"".join(paths)}"/>'
            f'<circle cx="{f(c[0])}" cy="{f(c[1])}" r="{f(rc)}" fill="{MAGENTA}"/></g>'
            f'<path class="w" transform="translate({f(tx)} 0)" d="{wd}"/></svg>\n')

def app_icon(P, size=180, inset=0.6):
    """Apple touch icon: the mark at `inset` of a paper square (iOS rounds the corners)."""
    paths, c, rc, _ = geom(P)
    k = inset; o = 50 * (1 - k)
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect width="100" height="100" fill="{PAPER}"/>'
           f'<g transform="translate({f(o)} {f(o)}) scale({k})"><path fill="{SAKURA}" d="{"".join(paths)}"/>'
           f'<circle cx="{f(c[0])}" cy="{f(c[1])}" r="{f(rc)}" fill="{MAGENTA}"/></g></svg>')
    rsvg = shutil.which("rsvg-convert")
    if not rsvg:
        print("skipped app/apple-icon.png: rsvg-convert not found"); return
    subprocess.run([rsvg, "-w", str(size), "-h", str(size), "-o", os.path.join(ROOT, "app", "apple-icon.png")],
                   input=svg.encode(), check=True)

def main():
    write("sakura-mark.svg", mark_svg(MAIN))
    write("sakura-mark-mono.svg", mark_svg(MAIN, colour=False))
    write("sakura-mark-small.svg", mark_svg(SMALL))
    write("sakura-lockup.svg", lockup())
    write("favicon.svg", mark_svg(SMALL))
    shutil.copy(os.path.join(OUT, "favicon.svg"), os.path.join(ROOT, "app", "icon.svg"))
    app_icon(MAIN)
    # Constants for components/brand.tsx: one upright petal around (50, 50) plus the optical offset.
    for name, P in (("main", MAIN), ("small", SMALL)):
        _, c, rc, _ = geom(P)
        print(f'{name}: petal="{build(P, c=(50, 50))[0][0]}" core={rc} dy={f(c[1] - 50)}')


if __name__ == "__main__":
    main()
