#!/usr/bin/env python3
import argparse
import math
import subprocess
import tempfile
from pathlib import Path

WIDTH, HEIGHT = 1000, 420
EDGE_CANDIDATES = [
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]
LIGHT = (0.35, -0.55, 0.76)


def read_obj(path: Path):
    vertices, faces = [], []
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "v":
            vertices.append(tuple(float(c) for c in parts[1:4]))
        elif parts[0] == "f":
            faces.append([int(p.split("/")[0]) - 1 for p in parts[1:]])
    return vertices, faces


def rotate(p, yaw: float, pitch: float):
    x, y, z = p
    cy, sy = math.cos(yaw), math.sin(yaw)
    x, y = cy * x - sy * y, sy * x + cy * y
    cp, sp = math.cos(pitch), math.sin(pitch)
    y, z = cp * y - sp * z, sp * y + cp * z
    return x, y, z


def normalise(v):
    length = math.sqrt(sum(c * c for c in v)) or 1.0
    return tuple(c / length for c in v)


def shaded_mesh(vertices, faces, cx: float, cy: float, size: float, yaw: float, pitch: float, edge_width: float) -> str:
    lo = [min(v[k] for v in vertices) for k in range(3)]
    hi = [max(v[k] for v in vertices) for k in range(3)]
    centre = [(a + b) / 2 for a, b in zip(lo, hi)]
    scale = size / (max(b - a for a, b in zip(lo, hi)) or 1.0)
    view = [rotate(tuple((c - o) * scale for c, o in zip(v, centre)), yaw, pitch) for v in vertices]
    light = normalise(LIGHT)
    drawn = []
    for f in faces:
        pts = [view[i] for i in f]
        a, b, c = pts[0], pts[1], pts[2]
        u = [b[k] - a[k] for k in range(3)]
        w = [c[k] - a[k] for k in range(3)]
        n = normalise((u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]))
        if n[1] > 0:
            continue
        lambert = max(0.0, n[0] * light[0] + n[1] * light[1] + n[2] * light[2])
        shade = int(92 + 120 * lambert)
        depth = sum(p[1] for p in pts) / len(pts)
        screen = " ".join(f"{cx + p[0]:.1f},{cy - p[2]:.1f}" for p in pts)
        drawn.append((depth, shade, screen))
    drawn.sort(key=lambda d: -d[0])
    return "\n".join(
        f'<polygon points="{s}" fill="rgb({g},{g},{g})" stroke="#1c1c1c" stroke-opacity="0.55" '
        f'stroke-width="{edge_width}" stroke-linejoin="round"/>'
        for _, g, s in drawn
    )


def escape(text: str) -> str:
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def build_svg(left: str, right: str, left_label: str, right_label: str) -> str:
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">
<defs>
<linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="#474747"/><stop offset="1" stop-color="#2e2e2e"/>
</linearGradient>
<style>
.label {{ font-family: 'Segoe UI', sans-serif; font-size: 15px; fill: #d6d6d6; }}
</style>
</defs>
<rect width="{WIDTH}" height="{HEIGHT}" fill="url(#bg)"/>
<line x1="{WIDTH / 2}" y1="0" x2="{WIDTH / 2}" y2="{HEIGHT}" stroke="#262626" stroke-width="2"/>
{left}
{right}
<text x="18" y="30" class="label">{escape(left_label)}</text>
<text x="{WIDTH / 2 + 18}" y="30" class="label">{escape(right_label)}</text>
</svg>
'''


def render_png(svg_path: Path, png_path: Path, scale: int) -> None:
    edge = next((c for c in EDGE_CANDIDATES if Path(c).exists()), None)
    if edge is None:
        raise SystemExit("Edge not found; add its path to EDGE_CANDIDATES.")
    html = f"<!doctype html><meta charset=utf-8><style>html,body{{margin:0;padding:0}}</style>{svg_path.read_text(encoding='utf-8')}"
    with tempfile.TemporaryDirectory() as tmp:
        page = Path(tmp) / "cover.html"
        page.write_text(html, encoding="utf-8")
        subprocess.run([
            edge, "--headless=new", "--disable-gpu", "--hide-scrollbars",
            f"--force-device-scale-factor={scale}", f"--window-size={WIDTH},{HEIGHT}",
            f"--screenshot={png_path.resolve()}", page.as_uri(),
        ], check=True, timeout=120, capture_output=True)
    if not png_path.exists():
        raise SystemExit("render failed: no PNG produced")


def main():
    parser = argparse.ArgumentParser(description="Make a viewport-style before and after cover for a skpforge article.")
    parser.add_argument("--high", required=True, help="Input mesh drawn on the left")
    parser.add_argument("--low", required=True, help="Result mesh drawn on the right")
    parser.add_argument("--high-label", required=True, help="Plain label for the left mesh, with its real count")
    parser.add_argument("--low-label", required=True, help="Plain label for the right mesh, with its real count")
    parser.add_argument("--out", required=True, help="Article folder; writes cover.svg and cover.png there")
    parser.add_argument("--yaw", type=float, default=30.0)
    parser.add_argument("--pitch", type=float, default=32.0)
    parser.add_argument("--scale", type=int, default=2, help="Pixel density of the PNG")
    args = parser.parse_args()

    yaw, pitch = math.radians(args.yaw), math.radians(args.pitch)
    hv, hf = read_obj(Path(args.high))
    lv, lf = read_obj(Path(args.low))
    left = shaded_mesh(hv, hf, WIDTH * 0.25, HEIGHT * 0.54, 400, yaw, pitch, 0.35)
    right = shaded_mesh(lv, lf, WIDTH * 0.75, HEIGHT * 0.54, 400, yaw, pitch, 0.8)
    svg = build_svg(left, right, args.high_label, args.low_label)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    svg_path, png_path = out / "cover.svg", out / "cover.png"
    svg_path.write_text(svg, encoding="utf-8")
    render_png(svg_path, png_path, args.scale)
    print(f"wrote {svg_path} and {png_path} ({WIDTH * args.scale}x{HEIGHT * args.scale})")


if __name__ == "__main__":
    main()
