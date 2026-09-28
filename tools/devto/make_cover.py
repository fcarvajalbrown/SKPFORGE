#!/usr/bin/env python3
import argparse
import math
import subprocess
import tempfile
from pathlib import Path

WIDTH, HEIGHT = 1000, 420
ORANGE = "#F26B1D"
BLUE = "#3A8DDE"
EDGE_CANDIDATES = [
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]


def read_obj(path: Path) -> tuple[list[tuple[float, float, float]], list[list[int]]]:
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


def mesh_layer(vertices, faces, cx: float, cy: float, size: float, yaw: float, pitch: float) -> str:
    xs = [v[0] for v in vertices]
    ys = [v[1] for v in vertices]
    zs = [v[2] for v in vertices]
    centre = ((max(xs) + min(xs)) / 2, (max(ys) + min(ys)) / 2, (max(zs) + min(zs)) / 2)
    extent = max(max(xs) - min(xs), max(ys) - min(ys), max(zs) - min(zs)) or 1.0
    scale = size / extent
    projected = []
    for v in vertices:
        x, y, z = rotate(tuple(c - o for c, o in zip(v, centre)), yaw, pitch)
        projected.append((cx + x * scale, cy - z * scale, y))
    drawn = []
    for f in faces:
        pts = [projected[i] for i in f]
        (ax, ay, _), (bx, by, _), (qx, qy, _) = pts[0], pts[1], pts[2]
        facing = (bx - ax) * (qy - ay) - (by - ay) * (qx - ax)
        depth = sum(p[2] for p in pts) / len(pts)
        drawn.append((depth, facing, pts))
    drawn.sort(key=lambda d: -d[0])
    out = []
    for _, facing, pts in drawn:
        path = " ".join(f"{x:.1f},{y:.1f}" for x, y, _ in pts)
        if facing > 0:
            out.append(f'<polygon points="{path}" fill="#0b0f16" stroke="{BLUE}" stroke-opacity="0.25" stroke-width="0.6"/>')
        else:
            out.append(f'<polygon points="{path}" fill="#141b26" fill-opacity="0.92" stroke="{ORANGE}" stroke-width="0.9"/>')
    return "\n".join(out)


def grid_floor() -> str:
    horizon, vanish_x = 250, 720
    lines = []
    for i in range(-24, 25):
        x = vanish_x + i * 90
        lines.append(f'<line x1="{vanish_x}" y1="{horizon}" x2="{x}" y2="{HEIGHT + 40}"/>')
    y, step = horizon + 6, 4.0
    while y < HEIGHT:
        lines.append(f'<line x1="0" y1="{y:.1f}" x2="{WIDTH}" y2="{y:.1f}"/>')
        step *= 1.35
        y += step
    return f'<g stroke="{BLUE}" stroke-opacity="0.18" stroke-width="1">{"".join(lines)}</g>'


def brackets(x: float, y: float, w: float, h: float, arm: float = 18) -> str:
    corners = [
        (x, y, arm, 0, 0, arm), (x + w, y, -arm, 0, 0, arm),
        (x, y + h, arm, 0, 0, -arm), (x + w, y + h, -arm, 0, 0, -arm),
    ]
    parts = []
    for px, py, dx1, dy1, dx2, dy2 in corners:
        parts.append(f'<path d="M{px + dx1},{py + dy1} L{px},{py} L{px + dx2},{py + dy2}"/>')
    return f'<g fill="none" stroke="{ORANGE}" stroke-width="2.5">{"".join(parts)}</g>'


def escape(text: str) -> str:
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def build_svg(mesh: str, title: list[str], kicker: str, hud: list[str]) -> str:
    title_lines = "".join(
        f'<text x="56" y="{150 + i * 62}" class="title">{escape(line)}</text>' for i, line in enumerate(title)
    )
    hud_lines = "".join(
        f'<text x="{WIDTH - 60}" y="{HEIGHT - 96 + i * 22}" class="hud" text-anchor="end">{escape(line)}</text>'
        for i, line in enumerate(hud)
    )
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">
<defs>
<radialGradient id="bg" cx="70%" cy="45%" r="80%">
<stop offset="0" stop-color="#1b2433"/><stop offset="1" stop-color="#07090d"/>
</radialGradient>
<filter id="glow" x="-20%" y="-20%" width="140%" height="140%">
<feGaussianBlur stdDeviation="3" result="blur"/>
<feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge>
</filter>
<pattern id="scan" width="4" height="4" patternUnits="userSpaceOnUse">
<rect width="4" height="1" fill="#000" fill-opacity="0.18"/>
</pattern>
<style>
.title {{ font-family: Bahnschrift, 'Segoe UI', sans-serif; font-weight: 700; font-stretch: condensed; font-size: 54px; fill: #f4f6fa; letter-spacing: 1px; }}
.kicker {{ font-family: Consolas, monospace; font-size: 17px; fill: {ORANGE}; letter-spacing: 4px; }}
.hud {{ font-family: Consolas, monospace; font-size: 15px; fill: #9fb4cc; }}
</style>
</defs>
<rect width="{WIDTH}" height="{HEIGHT}" fill="url(#bg)"/>
{grid_floor()}
<g filter="url(#glow)">{mesh}</g>
<rect width="{WIDTH}" height="{HEIGHT}" fill="url(#scan)"/>
<rect x="40" y="70" width="6" height="{len(title) * 62 + 10}" fill="{ORANGE}"/>
<text x="56" y="92" class="kicker">{escape(kicker)}</text>
{title_lines}
{brackets(24, 24, WIDTH - 48, HEIGHT - 48)}
{hud_lines}
</svg>
'''


def render_png(svg_path: Path, png_path: Path, scale: int) -> None:
    edge = next((c for c in EDGE_CANDIDATES if Path(c).exists()), None)
    if edge is None:
        raise SystemExit("Edge not found; add its path to EDGE_CANDIDATES.")
    html = f"<!doctype html><meta charset=utf-8><style>html,body{{margin:0;padding:0;background:#07090d}}</style>{svg_path.read_text(encoding='utf-8')}"
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
    parser = argparse.ArgumentParser(description="Make a game-dev style cover for a skpforge article.")
    parser.add_argument("--obj", required=True, help="Quad mesh to draw as the centrepiece")
    parser.add_argument("--title", required=True, help="Title, with | between lines")
    parser.add_argument("--kicker", default="SKPFORGE", help="Small label above the title")
    parser.add_argument("--hud", default="", help="Readout lines, with | between lines")
    parser.add_argument("--out", required=True, help="Article folder; writes cover.svg and cover.png there")
    parser.add_argument("--yaw", type=float, default=35.0)
    parser.add_argument("--pitch", type=float, default=28.0)
    parser.add_argument("--scale", type=int, default=2, help="Pixel density of the PNG")
    args = parser.parse_args()

    vertices, faces = read_obj(Path(args.obj))
    mesh = mesh_layer(vertices, faces, 775, 200, 360, math.radians(args.yaw), math.radians(args.pitch))
    hud = [line for line in args.hud.split("|") if line]
    svg = build_svg(mesh, args.title.split("|"), args.kicker, hud)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    svg_path, png_path = out / "cover.svg", out / "cover.png"
    svg_path.write_text(svg, encoding="utf-8")
    render_png(svg_path, png_path, args.scale)
    print(f"wrote {svg_path} and {png_path} ({WIDTH * args.scale}x{HEIGHT * args.scale})")


if __name__ == "__main__":
    main()
