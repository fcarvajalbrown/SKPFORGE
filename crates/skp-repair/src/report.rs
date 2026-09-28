use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EdgeCounts {
    pub open: usize,
    pub manifold: usize,
    pub non_manifold: usize,
    pub inconsistent: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RepairReport {
    pub positions_in: usize,
    pub positions_out: usize,
    pub triangles_in: usize,
    pub triangles_out: usize,
    pub positions_welded: usize,
    pub edges_after_weld: EdgeCounts,
    pub degenerates_dropped: usize,
    pub degenerates_collapsed: usize,
    pub needle_neighbours_split: usize,
    pub duplicates_dropped: usize,
    pub triangles_turned_over: usize,
    pub components: usize,
    pub closed_components: usize,
    pub interior_patches_culled: usize,
    pub interior_triangles_culled: usize,
    pub coplanar_vertices_removed: usize,
    pub coplanar_triangles_removed: usize,
    pub edges_out: EdgeCounts,
}

impl fmt::Display for EdgeCounts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} open, {} manifold ({} wound inconsistently), {} non-manifold",
            self.open, self.manifold, self.inconsistent, self.non_manifold
        )
    }
}

impl fmt::Display for RepairReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "positions              {} -> {}",
            self.positions_in, self.positions_out
        )?;
        writeln!(
            f,
            "triangles              {} -> {}",
            self.triangles_in, self.triangles_out
        )?;
        writeln!(f, "positions welded       {}", self.positions_welded)?;
        writeln!(f, "edges after weld       {}", self.edges_after_weld)?;
        writeln!(
            f,
            "degenerates dropped    {} ({} collapsed, {} needles, {} neighbours split)",
            self.degenerates_dropped,
            self.degenerates_collapsed,
            self.degenerates_dropped - self.degenerates_collapsed.min(self.degenerates_dropped),
            self.needle_neighbours_split
        )?;
        writeln!(f, "duplicates dropped     {}", self.duplicates_dropped)?;
        writeln!(
            f,
            "components             {} ({} closed)",
            self.components, self.closed_components
        )?;
        writeln!(f, "triangles turned over  {}", self.triangles_turned_over)?;
        writeln!(
            f,
            "interior culled        {} triangles in {} patches",
            self.interior_triangles_culled, self.interior_patches_culled
        )?;
        writeln!(
            f,
            "coplanar merge         {} triangles, {} vertices removed",
            self.coplanar_triangles_removed, self.coplanar_vertices_removed
        )?;
        write!(f, "edges out              {}", self.edges_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operation_gets_its_own_line() {
        let report = RepairReport {
            positions_in: 6,
            positions_out: 4,
            triangles_in: 2,
            triangles_out: 2,
            positions_welded: 2,
            edges_after_weld: EdgeCounts {
                open: 4,
                manifold: 1,
                non_manifold: 0,
                inconsistent: 1,
            },
            degenerates_dropped: 10,
            degenerates_collapsed: 2,
            needle_neighbours_split: 8,
            duplicates_dropped: 3,
            triangles_turned_over: 4,
            components: 5,
            closed_components: 2,
            interior_patches_culled: 1,
            interior_triangles_culled: 6,
            coplanar_vertices_removed: 7,
            coplanar_triangles_removed: 14,
            edges_out: EdgeCounts {
                open: 3,
                manifold: 2,
                non_manifold: 1,
                inconsistent: 0,
            },
        };
        let text = report.to_string();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines,
            vec![
                "positions              6 -> 4",
                "triangles              2 -> 2",
                "positions welded       2",
                "edges after weld       4 open, 1 manifold (1 wound inconsistently), 0 non-manifold",
                "degenerates dropped    10 (2 collapsed, 8 needles, 8 neighbours split)",
                "duplicates dropped     3",
                "components             5 (2 closed)",
                "triangles turned over  4",
                "interior culled        6 triangles in 1 patches",
                "coplanar merge         14 triangles, 7 vertices removed",
                "edges out              3 open, 2 manifold (0 wound inconsistently), 1 non-manifold",
            ]
        );
    }
}
