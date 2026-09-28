use crate::error::RetopoError;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

#[derive(Debug, Clone, PartialEq)]
pub struct SymmetricMatrix {
    n: usize,
    lower: Vec<Vec<(u32, f64)>>,
}

impl SymmetricMatrix {
    pub fn from_triplets(n: usize, triplets: &[(u32, u32, f64)]) -> SymmetricMatrix {
        let mut lower: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n];
        for &(row, col, value) in triplets {
            if row >= col {
                lower[col as usize].push((row, value));
            }
        }
        for column in &mut lower {
            column.sort_by_key(|&(row, _)| row);
            column.dedup_by(|later, kept| {
                if later.0 == kept.0 {
                    kept.1 += later.1;
                    true
                } else {
                    false
                }
            });
        }
        SymmetricMatrix { n, lower }
    }

    pub fn size(&self) -> usize {
        self.n
    }

    fn neighbours(&self) -> Vec<Vec<u32>> {
        let mut adj: Vec<Vec<u32>> = vec![Vec::new(); self.n];
        for (col, column) in self.lower.iter().enumerate() {
            for &(row, _) in column {
                if row as usize != col {
                    adj[col].push(row);
                    adj[row as usize].push(col as u32);
                }
            }
        }
        for list in &mut adj {
            list.sort_unstable();
            list.dedup();
        }
        adj
    }
}

fn merge_without(a: &[u32], b: &[u32], skip: [u32; 2]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        let next = match (a.get(i), b.get(j)) {
            (Some(&x), Some(&y)) if x == y => {
                i += 1;
                j += 1;
                x
            }
            (Some(&x), Some(&y)) if x < y => {
                i += 1;
                x
            }
            (Some(_), Some(&y)) => {
                j += 1;
                y
            }
            (Some(&x), None) => {
                i += 1;
                x
            }
            (None, Some(&y)) => {
                j += 1;
                y
            }
            (None, None) => unreachable!(),
        };
        if !skip.contains(&next) {
            out.push(next);
        }
    }
    out
}

pub fn minimum_degree_order(a: &SymmetricMatrix) -> Vec<u32> {
    let mut adj = a.neighbours();
    let mut eliminated = vec![false; a.n];
    let mut heap: BinaryHeap<Reverse<(usize, u32)>> = adj
        .iter()
        .enumerate()
        .map(|(v, list)| Reverse((list.len(), v as u32)))
        .collect();
    let mut order = Vec::with_capacity(a.n);
    while let Some(Reverse((degree, v))) = heap.pop() {
        let vi = v as usize;
        if eliminated[vi] || degree != adj[vi].len() {
            continue;
        }
        eliminated[vi] = true;
        order.push(v);
        let clique = std::mem::take(&mut adj[vi]);
        for &u in &clique {
            let ui = u as usize;
            adj[ui] = merge_without(&adj[ui], &clique, [u, v]);
            heap.push(Reverse((adj[ui].len(), u)));
        }
    }
    order
}

#[derive(Debug, Clone)]
pub struct Cholesky {
    perm: Vec<u32>,
    diag: Vec<f64>,
    columns: Vec<Vec<(u32, f64)>>,
}

impl Cholesky {
    pub fn factor(a: &SymmetricMatrix) -> Result<Cholesky, RetopoError> {
        Cholesky::factor_in_order(a, minimum_degree_order(a))
    }

    pub fn factor_in_order(a: &SymmetricMatrix, perm: Vec<u32>) -> Result<Cholesky, RetopoError> {
        let n = a.n;
        let mut pinv = vec![0u32; n];
        for (k, &p) in perm.iter().enumerate() {
            pinv[p as usize] = k as u32;
        }
        let mut upper: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n];
        for (col, column) in a.lower.iter().enumerate() {
            for &(row, value) in column {
                let (pr, pc) = (pinv[row as usize], pinv[col]);
                upper[pr.max(pc) as usize].push((pr.min(pc), value));
            }
        }

        const NONE: u32 = u32::MAX;
        let mut parent = vec![NONE; n];
        let mut ancestor = vec![NONE; n];
        for (k, column) in upper.iter().enumerate() {
            for &(row, _) in column {
                let mut i = row;
                while i != NONE && (i as usize) < k {
                    let next = ancestor[i as usize];
                    ancestor[i as usize] = k as u32;
                    if next == NONE {
                        parent[i as usize] = k as u32;
                    }
                    i = next;
                }
            }
        }

        let mut diag = vec![0.0; n];
        let mut columns: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n];
        let mut x = vec![0.0; n];
        let mut mark = vec![NONE; n];
        let mut paths: Vec<Vec<u32>> = Vec::new();
        for k in 0..n {
            paths.clear();
            mark[k] = k as u32;
            for &(row, value) in &upper[k] {
                x[row as usize] += value;
                let mut i = row;
                let mut path = Vec::new();
                while mark[i as usize] != k as u32 {
                    path.push(i);
                    mark[i as usize] = k as u32;
                    i = parent[i as usize];
                }
                if !path.is_empty() {
                    paths.push(path);
                }
            }
            let mut d = x[k];
            x[k] = 0.0;
            for path in paths.iter().rev() {
                for &i in path {
                    let i = i as usize;
                    let lki = x[i] / diag[i];
                    x[i] = 0.0;
                    for &(r, v) in &columns[i] {
                        x[r as usize] -= v * lki;
                    }
                    d -= lki * lki;
                    columns[i].push((k as u32, lki));
                }
            }
            if d.is_nan() || d <= 0.0 {
                return Err(RetopoError::NotPositiveDefinite { column: k as u32 });
            }
            diag[k] = d.sqrt();
        }
        Ok(Cholesky {
            perm,
            diag,
            columns,
        })
    }

    pub fn non_zeros(&self) -> usize {
        self.diag.len() + self.columns.iter().map(Vec::len).sum::<usize>()
    }

    pub fn solve(&self, b: &[f64]) -> Vec<f64> {
        let n = self.diag.len();
        let mut z: Vec<f64> = self.perm.iter().map(|&p| b[p as usize]).collect();
        for j in 0..n {
            z[j] /= self.diag[j];
            let zj = z[j];
            for &(r, v) in &self.columns[j] {
                z[r as usize] -= v * zj;
            }
        }
        for j in (0..n).rev() {
            let mut w = z[j];
            for &(r, v) in &self.columns[j] {
                w -= v * z[r as usize];
            }
            z[j] = w / self.diag[j];
        }
        let mut out = vec![0.0; n];
        for (k, &p) in self.perm.iter().enumerate() {
            out[p as usize] = z[k];
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full(rows: &[&[f64]]) -> Vec<(u32, u32, f64)> {
        let mut t = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            for (j, &v) in row.iter().enumerate() {
                if v != 0.0 {
                    t.push((i as u32, j as u32, v));
                }
            }
        }
        t
    }

    fn grid_laplacian(side: u32) -> SymmetricMatrix {
        let n = side * side;
        let mut t = Vec::new();
        for y in 0..side {
            for x in 0..side {
                let i = y * side + x;
                t.push((i, i, 1.0));
                let mut link = |j: u32| {
                    t.push((i, i, 1.0));
                    t.push((j, j, 1.0));
                    t.push((i, j, -1.0));
                    t.push((j, i, -1.0));
                };
                if x + 1 < side {
                    link(i + 1);
                }
                if y + 1 < side {
                    link(i + side);
                }
            }
        }
        SymmetricMatrix::from_triplets(n as usize, &t)
    }

    fn multiply(a: &SymmetricMatrix, x: &[f64]) -> Vec<f64> {
        let mut y = vec![0.0; a.n];
        for (col, column) in a.lower.iter().enumerate() {
            for &(row, v) in column {
                let row = row as usize;
                y[row] += v * x[col];
                if row != col {
                    y[col] += v * x[row];
                }
            }
        }
        y
    }

    #[test]
    fn a_small_system_solves_exactly() {
        let a = SymmetricMatrix::from_triplets(
            3,
            &full(&[&[4.0, 2.0, 0.0], &[2.0, 5.0, 1.0], &[0.0, 1.0, 3.0]]),
        );
        let x = Cholesky::factor(&a).unwrap().solve(&[8.0, 15.0, 11.0]);
        for (got, want) in x.iter().zip([1.0, 2.0, 3.0]) {
            assert!((got - want).abs() < 1e-12);
        }
    }

    #[test]
    fn only_the_lower_triangle_is_read_and_duplicates_add() {
        let t = [
            (0, 0, 2.0),
            (0, 0, 2.0),
            (1, 0, 2.0),
            (0, 1, 99.0),
            (1, 1, 5.0),
        ];
        let a = SymmetricMatrix::from_triplets(2, &t);
        let x = Cholesky::factor(&a).unwrap().solve(&[8.0, 12.0]);
        assert!((x[0] - 1.0).abs() < 1e-12 && (x[1] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn an_indefinite_matrix_is_reported_not_solved() {
        let a = SymmetricMatrix::from_triplets(2, &full(&[&[1.0, 2.0], &[2.0, 1.0]]));
        assert!(matches!(
            Cholesky::factor_in_order(&a, vec![0, 1]),
            Err(RetopoError::NotPositiveDefinite { column: 1 })
        ));
    }

    #[test]
    fn a_grid_laplacian_solves_to_rounding() {
        let a = grid_laplacian(20);
        let want: Vec<f64> = (0..a.n).map(|i| (i as f64 * 0.37).sin()).collect();
        let b = multiply(&a, &want);
        let x = Cholesky::factor(&a).unwrap().solve(&b);
        let worst = x
            .iter()
            .zip(&want)
            .map(|(g, w)| (g - w).abs())
            .fold(0.0, f64::max);
        assert!(worst < 1e-10, "worst {worst}");
    }

    #[test]
    fn minimum_degree_is_a_permutation_that_fills_less_than_the_natural_order() {
        let a = grid_laplacian(20);
        let order = minimum_degree_order(&a);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..400).collect::<Vec<u32>>());
        let md = Cholesky::factor_in_order(&a, order).unwrap().non_zeros();
        let natural = Cholesky::factor_in_order(&a, (0..400).collect())
            .unwrap()
            .non_zeros();
        assert!(md < natural, "minimum degree {md}, natural {natural}");
    }
}
