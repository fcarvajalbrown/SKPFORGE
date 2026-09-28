use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RepairReport {
    pub positions_in: usize,
    pub positions_out: usize,
    pub triangles_in: usize,
    pub triangles_out: usize,
    pub positions_welded: usize,
    pub degenerates_dropped: usize,
    pub duplicates_dropped: usize,
    pub triangles_turned_over: usize,
    pub components: usize,
    pub closed_components: usize,
    pub interior_patches_culled: usize,
    pub interior_triangles_culled: usize,
    pub coplanar_vertices_removed: usize,
    pub coplanar_triangles_removed: usize,
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
        writeln!(f, "degenerates dropped    {}", self.degenerates_dropped)?;
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
        write!(
            f,
            "coplanar merge         {} triangles, {} vertices removed",
            self.coplanar_triangles_removed, self.coplanar_vertices_removed
        )
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
            degenerates_dropped: 1,
            duplicates_dropped: 3,
            triangles_turned_over: 4,
            components: 5,
            closed_components: 2,
            interior_patches_culled: 1,
            interior_triangles_culled: 6,
            coplanar_vertices_removed: 7,
            coplanar_triangles_removed: 14,
        };
        let text = report.to_string();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines,
            vec![
                "positions              6 -> 4",
                "triangles              2 -> 2",
                "positions welded       2",
                "degenerates dropped    1",
                "duplicates dropped     3",
                "components             5 (2 closed)",
                "triangles turned over  4",
                "interior culled        6 triangles in 1 patches",
                "coplanar merge         14 triangles, 7 vertices removed",
            ]
        );
    }
}
