#[derive(Debug, Clone, Default)]
pub struct DisjointTree {
    parent: Vec<usize>,
    rank: Vec<usize>,
    indices: Vec<usize>,
    indices_to_parent: Vec<usize>,
}

impl DisjointTree {
    pub fn new(n: usize) -> DisjointTree {
        DisjointTree {
            parent: (0..n).collect(),
            rank: vec![1; n],
            indices: Vec::new(),
            indices_to_parent: Vec::new(),
        }
    }

    pub fn parent(&mut self, x: usize) -> usize {
        let mut root = x;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        let mut node = x;
        while self.parent[node] != root {
            let next = self.parent[node];
            self.parent[node] = root;
            node = next;
        }
        root
    }

    pub fn merge_from_to(&mut self, x: usize, y: usize) {
        let px = self.parent(x);
        let py = self.parent(y);
        if px == py {
            return;
        }
        self.rank[py] += self.rank[px];
        self.parent[px] = py;
    }

    pub fn merge(&mut self, x: usize, y: usize) {
        let px = self.parent(x);
        let py = self.parent(y);
        if px == py {
            return;
        }
        if self.rank[px] < self.rank[py] {
            self.rank[py] += self.rank[px];
            self.parent[px] = py;
        } else {
            self.rank[px] += self.rank[py];
            self.parent[py] = px;
        }
    }

    pub fn build_compact_parent(&mut self) {
        let n = self.parent.len();
        let mut compact = vec![0; n];
        self.indices_to_parent.clear();
        for (i, slot) in compact.iter_mut().enumerate() {
            if self.parent[i] == i {
                *slot = self.indices_to_parent.len();
                self.indices_to_parent.push(i);
            }
        }
        self.indices = (0..n).map(|i| compact[self.parent(i)]).collect();
    }

    pub fn index(&self, x: usize) -> usize {
        self.indices[x]
    }

    pub fn index_to_parent(&self, x: usize) -> usize {
        self.indices_to_parent[x]
    }

    pub fn compact_num(&self) -> usize {
        self.indices_to_parent.len()
    }
}

#[derive(Debug, Clone, Default)]
pub struct DisjointOrientTree {
    parent: Vec<(usize, i32)>,
    rank: Vec<usize>,
    indices: Vec<usize>,
    compact_num: usize,
}

impl DisjointOrientTree {
    pub fn new(n: usize) -> DisjointOrientTree {
        DisjointOrientTree {
            parent: (0..n).map(|i| (i, 0)).collect(),
            rank: vec![1; n],
            indices: Vec::new(),
            compact_num: 0,
        }
    }

    pub fn parent(&mut self, j: usize) -> usize {
        let mut path = Vec::new();
        let mut root = j;
        while self.parent[root].0 != root {
            path.push(root);
            root = self.parent[root].0;
        }
        for &node in path.iter().rev() {
            let up = self.parent[node].0;
            let turned = (self.parent[node].1 + self.parent[up].1) % 4;
            self.parent[node] = (root, turned);
        }
        root
    }

    pub fn orient(&self, j: usize) -> i32 {
        let mut total = 0;
        let mut node = j;
        loop {
            let (up, turn) = self.parent[node];
            if up == node {
                return (total + turn) % 4;
            }
            total = (total + turn) % 4;
            node = up;
        }
    }

    pub fn merge_from_to(&mut self, v0: usize, v1: usize, orient0: i32, orient1: i32) {
        let p0 = self.parent(v0);
        let p1 = self.parent(v1);
        if p0 == p1 {
            return;
        }
        let orientp0 = self.orient(v0);
        let orientp1 = self.orient(v1);
        self.rank[p1] += self.rank[p0];
        self.parent[p0] = (p1, (orient0 - orient1 + orientp1 - orientp0 + 8) % 4);
    }

    pub fn merge(&mut self, v0: usize, v1: usize, orient0: i32, orient1: i32) {
        let p0 = self.parent(v0);
        let p1 = self.parent(v1);
        if p0 == p1 {
            return;
        }
        let orientp0 = self.orient(v0);
        let orientp1 = self.orient(v1);
        if self.rank[p1] < self.rank[p0] {
            self.rank[p0] += self.rank[p1];
            self.parent[p1] = (p0, (orient1 - orient0 + orientp0 - orientp1 + 8) % 4);
        } else {
            self.rank[p1] += self.rank[p0];
            self.parent[p0] = (p1, (orient0 - orient1 + orientp1 - orientp0 + 8) % 4);
        }
    }

    pub fn build_compact_parent(&mut self) {
        let n = self.parent.len();
        let mut compact = vec![0; n];
        self.compact_num = 0;
        for (i, slot) in compact.iter_mut().enumerate() {
            if self.parent[i].0 == i {
                *slot = self.compact_num;
                self.compact_num += 1;
            }
        }
        self.indices = (0..n).map(|i| compact[self.parent(i)]).collect();
    }

    pub fn index(&self, x: usize) -> usize {
        self.indices[x]
    }

    pub fn compact_num(&self) -> usize {
        self.compact_num
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_hangs_the_smaller_set_under_the_larger() {
        let mut t = DisjointTree::new(4);
        t.merge(0, 1);
        t.merge(2, 0);
        assert_eq!(t.parent(2), 0);
        assert_eq!(t.parent(1), 0);
        assert_eq!(t.parent(3), 3);
    }

    #[test]
    fn merge_from_to_always_hangs_the_first_under_the_second() {
        let mut t = DisjointTree::new(3);
        t.merge_from_to(0, 1);
        t.merge_from_to(1, 2);
        assert_eq!(t.parent(0), 2);
    }

    #[test]
    fn compact_indices_number_the_roots_in_vertex_order() {
        let mut t = DisjointTree::new(5);
        t.merge_from_to(0, 3);
        t.merge_from_to(4, 1);
        t.build_compact_parent();
        assert_eq!(t.compact_num(), 3);
        let got: Vec<usize> = (0..5).map(|i| t.index(i)).collect();
        assert_eq!(got, [2, 0, 1, 2, 0]);
        assert_eq!(t.index_to_parent(2), 3);
    }

    #[test]
    fn orientations_add_up_along_the_path_to_the_root() {
        let mut t = DisjointOrientTree::new(3);
        t.merge_from_to(0, 1, 1, 0);
        t.merge_from_to(1, 2, 2, 0);
        assert_eq!(t.orient(0), 3);
        assert_eq!(t.orient(1), 2);
        assert_eq!(t.orient(2), 0);
        assert_eq!(t.parent(0), 2);
        assert_eq!(t.orient(0), 3);
    }

    #[test]
    fn orient_merge_keeps_relative_orientation_between_members() {
        let mut t = DisjointOrientTree::new(4);
        t.merge(0, 1, 1, 0);
        t.merge(2, 3, 3, 1);
        t.merge(1, 3, 0, 2);
        let rel = |t: &DisjointOrientTree, a: usize, b: usize| (t.orient(a) - t.orient(b) + 4) % 4;
        assert_eq!(rel(&t, 0, 1), 1);
        assert_eq!(rel(&t, 2, 3), 2);
        assert_eq!(rel(&t, 1, 3), 2);
        t.build_compact_parent();
        assert_eq!(t.compact_num(), 1);
    }
}
