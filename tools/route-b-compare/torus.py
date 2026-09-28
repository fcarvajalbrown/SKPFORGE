import math
import sys


def torus(major, minor, rings, sides):
    vertices = []
    for i in range(rings):
        u = 2 * math.pi * i / rings
        for j in range(sides):
            v = 2 * math.pi * j / sides
            r = major + minor * math.cos(v)
            vertices.append((r * math.cos(u), r * math.sin(u), minor * math.sin(v)))
    faces = []
    for i in range(rings):
        for j in range(sides):
            a = i * sides + j
            b = ((i + 1) % rings) * sides + j
            c = ((i + 1) % rings) * sides + (j + 1) % sides
            d = i * sides + (j + 1) % sides
            faces.append((a, b, c))
            faces.append((a, c, d))
    return vertices, faces


def main():
    if len(sys.argv) != 4:
        sys.exit("usage: torus.py <out.obj> <rings> <sides>")
    out, rings, sides = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    vertices, faces = torus(100.0, 35.0, rings, sides)
    with open(out, "w", newline="\n") as f:
        for x, y, z in vertices:
            f.write(f"v {x:.9f} {y:.9f} {z:.9f}\n")
        for a, b, c in faces:
            f.write(f"f {a + 1} {b + 1} {c + 1}\n")


if __name__ == "__main__":
    main()
