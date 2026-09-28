use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RepairReport {
    pub positions_in: usize,
    pub positions_out: usize,
    pub triangles_in: usize,
    pub triangles_out: usize,
    pub positions_welded: usize,
    pub degenerates_dropped: usize,
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
        write!(f, "degenerates dropped    {}", self.degenerates_dropped)
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
        };
        let text = report.to_string();
        assert!(text.contains("positions              6 -> 4"));
        assert!(text.contains("positions welded       2"));
        assert!(text.contains("degenerates dropped    1"));
    }
}
