import collections
import sys


def read(path):
    vertices = 0
    faces = []
    with open(path) as f:
        for line in f:
            parts = line.split()
            if not parts:
                continue
            if parts[0] == "v":
                vertices += 1
            elif parts[0] == "f":
                faces.append([int(p.split("/")[0]) - 1 for p in parts[1:]])
    return vertices, faces


def stats(path):
    vertices, faces = read(path)
    sizes = collections.Counter(len(f) for f in faces)
    valence = collections.Counter()
    edges = collections.Counter()
    for f in faces:
        for k in range(len(f)):
            a, b = f[k], f[(k + 1) % len(f)]
            edges[(min(a, b), max(a, b))] += 1
    for (a, b), _ in edges.items():
        valence[a] += 1
        valence[b] += 1
    histogram = collections.Counter(valence[v] for v in range(vertices))
    open_edges = sum(1 for n in edges.values() if n == 1)
    over_shared = sum(1 for n in edges.values() if n > 2)
    euler = vertices - len(edges) + len(faces)
    irregular = sum(n for k, n in histogram.items() if k != 4)
    print(f"{path}")
    print(f"  vertices {vertices}  faces {len(faces)}  by corners {dict(sorted(sizes.items()))}")
    print(f"  edges {len(edges)}  open {open_edges}  shared by 3+ {over_shared}  euler {euler}")
    print(f"  valence {dict(sorted(histogram.items()))}  irregular {irregular}")


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: stats.py <mesh.obj> [...]")
    for path in sys.argv[1:]:
        stats(path)


if __name__ == "__main__":
    main()
