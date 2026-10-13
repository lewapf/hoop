#!/usr/bin/env python3
"""Generates Hoop's own frame, badge, banner and ornament symbols as
one-color SVGs on a 256 grid, filled with currentColor like Phosphor's."""
import math, os, sys
OUT = sys.argv[1]
C = 128.0
def f(v): return f"{v:.1f}".rstrip("0").rstrip(".")
def path(pts, close=True):
    d = "M" + " L".join(f"{f(x)},{f(y)}" for x, y in pts)
    return d + ("Z" if close else "")
def svg(name, body):
    open(os.path.join(OUT, name + ".svg"), "w").write(
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="currentColor">{body}</svg>\n')
def fill(*subpaths, evenodd=True):
    rule = ' fill-rule="evenodd"' if evenodd else ""
    return f'<path{rule} d="{"".join(subpaths)}"/>'
def circle(r, cx=C, cy=C, n=120, rx=None):
    rx = rx or r
    return [(cx + rx * math.cos(2 * math.pi * k / n), cy + r * math.sin(2 * math.pi * k / n)) for k in range(n)]
def rrect(w, h, r, cx=C, cy=C, n=10):
    pts = []
    for (qx, qy, a0) in [(w/2-r, h/2-r, 0), (-w/2+r, h/2-r, 90), (-w/2+r, -h/2+r, 180), (w/2-r, -h/2+r, 270)]:
        for k in range(n + 1):
            a = math.radians(a0 + 90 * k / n)
            pts.append((cx + qx + r * math.cos(a), cy + qy + r * math.sin(a)))
    return pts
def poly(n, r, rot=-90, cx=C, cy=C):
    return [(cx + r * math.cos(math.radians(rot + 360 * k / n)), cy + r * math.sin(math.radians(rot + 360 * k / n))) for k in range(n)]
def heart(s, cx=C, cy=C):
    pts = []
    for k in range(120):
        t = 2 * math.pi * k / 120
        x = 16 * math.sin(t) ** 3
        y = 13 * math.cos(t) - 5 * math.cos(2 * t) - 2 * math.cos(3 * t) - math.cos(4 * t)
        pts.append((cx + x * s / 17, cy - y * s / 17 - s * 0.08))
    return pts
def shield(w, h, cx=C, top=None):
    top = top if top is not None else C - h / 2
    pts = [(cx - w/2, top), (cx + w/2, top)]
    for k in range(25):
        t = k / 24
        pts.append((cx + w/2 * (1 - t ** 1.8), top + h * 0.35 + t * h * 0.65))
    for k in range(23, -1, -1):
        t = k / 24
        pts.append((cx - w/2 * (1 - t ** 1.8), top + h * 0.35 + t * h * 0.65))
    return pts
def leaf(x, y, ang, length, width):
    # A pointed leaf from (x, y) along angle `ang`.
    pts = []
    for k in range(13):
        t = k / 12
        pts.append((t * length, math.sin(math.pi * t) * width / 2))
    for k in range(11, 0, -1):
        t = k / 12
        pts.append((t * length, -math.sin(math.pi * t) * width / 2))
    c, s = math.cos(ang), math.sin(ang)
    return [(x + px * c - py * s, y + px * s + py * c) for px, py in pts]

# Frames: rings to stitch round a name or a motif.
svg("frame-circle", fill(path(circle(118)), path(circle(100))))
svg("frame-circle-double", fill(path(circle(120)), path(circle(108)), path(circle(100)), path(circle(92))))
svg("frame-square", fill(path(rrect(232, 232, 28)), path(rrect(200, 200, 14))))
svg("frame-oval", fill(path(circle(80, rx=122)), path(circle(64, rx=106))))
svg("frame-hexagon", fill(path(poly(6, 124, rot=-90)), path(poly(6, 104, rot=-90))))
svg("frame-diamond", fill(path(poly(4, 124)), path(poly(4, 100))))
svg("frame-heart", fill(path(heart(122)), path(heart(98, cy=C - 2))))
scallop = [(C + (112 + 8 * math.cos(16 * 2 * math.pi * k / 320)) * math.cos(2 * math.pi * k / 320),
            C + (112 + 8 * math.cos(16 * 2 * math.pi * k / 320)) * math.sin(2 * math.pi * k / 320)) for k in range(320)]
svg("frame-scallop", fill(path(scallop), path(circle(92))))
svg("frame-dots", "".join(fill(path(circle(7, C + 110 * math.cos(2 * math.pi * k / 28), C + 110 * math.sin(2 * math.pi * k / 28), n=24)), evenodd=False) for k in range(28)))
corners = []
for sx, sy in [(-1, -1), (1, -1), (1, 1), (-1, 1)]:
    a, b, w = 118, 70, 14
    pts = [(C + sx * a, C + sy * a), (C + sx * (a - b), C + sy * a), (C + sx * (a - b), C + sy * (a - w)), (C + sx * (a - w), C + sy * (a - w)),
           (C + sx * (a - w), C + sy * (a - b)), (C + sx * a, C + sy * (a - b))]
    corners.append(fill(path(pts), evenodd=False))
svg("frame-corners", "".join(corners))
tag = [(28, 76), (180, 76), (232, 128), (180, 180), (28, 180)]
svg("frame-tag", fill(path(tag), path(circle(9, cx=208, cy=128)), path([(44, 92), (164, 92), (188, 128), (164, 164), (44, 164)])))

# Badges and patches.
svg("badge-shield", fill(path(shield(208, 236, top=10)), path(shield(176, 196, top=28))))
seal = [(C + (122 if k % 2 == 0 else 108) * math.cos(math.pi * k / 24 - math.pi / 2),
         C + (122 if k % 2 == 0 else 108) * math.sin(math.pi * k / 24 - math.pi / 2)) for k in range(48)]
svg("badge-seal", fill(path(seal), path(circle(88))))
small = [(C + (84 if k % 2 == 0 else 74) * math.cos(math.pi * k / 20 - math.pi / 2),
          C - 26 + (84 if k % 2 == 0 else 74) * math.sin(math.pi * k / 20 - math.pi / 2)) for k in range(40)]
tails = [path([(84, 150), (112, 158), (92, 246), (76, 222), (56, 232)]), path([(172, 150), (144, 158), (164, 246), (180, 222), (200, 232)])]
svg("badge-rosette", fill(path(small), evenodd=False) + fill(*tails, evenodd=False))
svg("badge-circle", fill(path(circle(124)), path(circle(112)), path(circle(104))))
stamp = [(C + (120 + 5 * math.cos(40 * 2 * math.pi * k / 480)) * math.cos(2 * math.pi * k / 480),
          C + (120 + 5 * math.cos(40 * 2 * math.pi * k / 480)) * math.sin(2 * math.pi * k / 480)) for k in range(480)]
svg("badge-stamp", fill(path(stamp)))

# Banners for a name or a word.
band = [(40, 96), (216, 96), (216, 160), (40, 160)]
left_tail = [(4, 116), (52, 116), (52, 176), (4, 176), (22, 146)]
right_tail = [(252, 116), (204, 116), (204, 176), (252, 176), (234, 146)]
svg("banner-ribbon", fill(path(band), evenodd=False) + fill(path(left_tail), path(right_tail), evenodd=False))
def arch(r0, r1, a0, a1, cy):
    outer = [(C + r1 * math.cos(math.radians(a)), cy + r1 * math.sin(math.radians(a))) for a in [a0 + (a1 - a0) * k / 40 for k in range(41)]]
    inner = [(C + r0 * math.cos(math.radians(a)), cy + r0 * math.sin(math.radians(a))) for a in [a1 - (a1 - a0) * k / 40 for k in range(41)]]
    return outer + inner
svg("banner-arch", fill(path(arch(150, 196, 230, 310, 232)), evenodd=False)
    + fill(path([(16, 118), (60, 96), (76, 142), (36, 160), (40, 134)]), path([(240, 118), (196, 96), (180, 142), (220, 160), (216, 134)]), evenodd=False))
scroll = [(56, 100), (200, 100), (200, 156), (56, 156)]
svg("banner-scroll", fill(path(scroll), evenodd=False) + fill(path(circle(30, cx=48, cy=128)), path(circle(14, cx=48, cy=128)), path(circle(30, cx=208, cy=128)), path(circle(14, cx=208, cy=128))))
wave = []
for k in range(41):
    x = 16 + 224 * k / 40
    wave.append((x, 104 + 18 * math.sin(2 * math.pi * k / 40)))
for k in range(40, -1, -1):
    x = 16 + 224 * k / 40
    wave.append((x, 152 + 18 * math.sin(2 * math.pi * k / 40)))
svg("banner-wave", fill(path(wave), evenodd=False))

# Laurels.
R = 96
def stem_point(th):
    return (C + R * math.cos(th), C + 16 - R * math.sin(th))
def branch(th0, th1):
    # Leaves along the stem from th0 to th1, alternating out and in.
    out = []
    n = 11
    for k in range(n):
        th = th0 + (th1 - th0) * (k + 0.5) / n
        x, y = stem_point(th)
        x2, y2 = stem_point(th + (th1 - th0) * 0.01)
        d = math.atan2(y2 - y, x2 - x)
        for side in (1, -1):
            out.append(fill(path(leaf(x, y, d + side * 0.65, 36 - k, 15)), evenodd=False))
    return "".join(out)
def stem(th0, th1):
    pts = [stem_point(th0 + (th1 - th0) * k / 40) for k in range(41)]
    return f'<path d="{path(pts, False)}" fill="none" stroke="currentColor" stroke-width="6" stroke-linecap="round"/>'
svg("laurel-wreath", stem(math.radians(250), math.radians(105)) + stem(math.radians(-70), math.radians(75))
    + branch(math.radians(250), math.radians(105)) + branch(math.radians(-70), math.radians(75)))
straight = []
for k in range(7):
    x = 40 + k * 28
    straight.append(fill(path(leaf(x, 128, math.radians(-40), 44, 18)), path(leaf(x + 10, 128, math.radians(40), 44, 18)), evenodd=False))
svg("laurel-branch", '<path d="M24,128 L232,128" stroke="currentColor" stroke-width="6" stroke-linecap="round"/>' + "".join(straight))

# Ornaments.
def spiral(cx, cy, r0, turns, sign, start):
    pts = []
    for k in range(int(60 * turns) + 1):
        t = k / 60
        r = r0 * (1 - t / (turns + 0.4))
        a = start + sign * 2 * math.pi * t
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts
curl = spiral(150, 150, 70, 1.6, 1, math.pi * 0.75)
svg("flourish-corner", f'<path d="{path(curl, False)}" fill="none" stroke="currentColor" stroke-width="12" stroke-linecap="round"/>'
    + f'<path d="M24,24 L24,140 M24,24 L140,24" fill="none" stroke="currentColor" stroke-width="12" stroke-linecap="round"/>')
svg("flourish-swirl", f'<path d="{path(spiral(80, 128, 52, 1.4, -1, 0.0), False)}" fill="none" stroke="currentColor" stroke-width="12" stroke-linecap="round"/>'
    + f'<path d="{path(spiral(176, 128, 52, 1.4, 1, math.pi), False)}" fill="none" stroke="currentColor" stroke-width="12" stroke-linecap="round"/>')
div = [(8, 128), (100, 120), (128, 96), (156, 120), (248, 128), (156, 136), (128, 160), (100, 136)]
svg("divider-diamond", fill(path(div), path(poly(4, 14)), evenodd=True))
svg("divider-heart", fill(path([(8, 124), (92, 124), (92, 132), (8, 132)]), path([(164, 124), (248, 124), (248, 132), (164, 132)]), path(heart(56)), evenodd=False))
dots = "".join(fill(path(circle(6 + 4 * (1 - abs(k - 5) / 5), C + (k - 5) * 22, C, n=24)), evenodd=False) for k in range(11))
svg("divider-dots", dots)
