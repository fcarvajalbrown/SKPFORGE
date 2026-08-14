use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correspondence {
    low_triangle_count: usize,
    offsets: Vec<u32>,
    high: Vec<u32>,
}

impl Correspondence {
    pub fn low_triangle_count(&self) -> usize {
        self.low_triangle_count
    }

    pub fn pair_count(&self) -> usize {
        self.high.len()
    }

    pub fn is_empty(&self) -> bool {
        self.high.is_empty()
    }

    pub fn high_for(&self, low: u32) -> &[u32] {
        let index = low as usize;
        if index >= self.low_triangle_count {
            return &[];
        }
        let start = self.offsets[index] as usize;
        let end = self.offsets[index + 1] as usize;
        &self.high[start..end]
    }

    pub fn validate(&self) -> Result<(), CorrespondenceError> {
        if self.offsets.len() != self.low_triangle_count + 1 {
            return Err(CorrespondenceError::OffsetLengthMismatch {
                offsets: self.offsets.len(),
                expected: self.low_triangle_count + 1,
            });
        }
        for (low, pair) in self.offsets.windows(2).enumerate() {
            if pair[0] > pair[1] {
                return Err(CorrespondenceError::OffsetsNotMonotonic { low: low as u32 });
            }
            if pair[0] == pair[1] {
                return Err(CorrespondenceError::UnmappedLowTriangle { low: low as u32 });
            }
        }
        let trailing = self.offsets[self.low_triangle_count] as usize;
        if trailing != self.high.len() {
            return Err(CorrespondenceError::TrailingOffsetMismatch {
                offset: trailing,
                pairs: self.high.len(),
            });
        }
        Ok(())
    }
}

pub struct CorrespondenceBuilder {
    low_triangle_count: usize,
    pairs: Vec<(u32, u32)>,
}

impl CorrespondenceBuilder {
    pub fn new(low_triangle_count: usize) -> Self {
        CorrespondenceBuilder {
            low_triangle_count,
            pairs: Vec::new(),
        }
    }

    pub fn push(&mut self, low: u32, high: u32) {
        self.pairs.push((low, high));
    }

    pub fn build(self) -> Correspondence {
        let low_triangle_count = self.low_triangle_count;
        let mut pairs = self.pairs;
        pairs.sort_unstable();
        pairs.dedup();

        let mut offsets = vec![0u32; low_triangle_count + 1];
        for &(low, _) in &pairs {
            offsets[low as usize + 1] += 1;
        }
        let mut running = 0u32;
        for slot in offsets.iter_mut() {
            running += *slot;
            *slot = running;
        }

        let high = pairs.into_iter().map(|(_, h)| h).collect();

        Correspondence {
            low_triangle_count,
            offsets,
            high,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrespondenceError {
    UnmappedLowTriangle { low: u32 },
    OffsetLengthMismatch { offsets: usize, expected: usize },
    OffsetsNotMonotonic { low: u32 },
    TrailingOffsetMismatch { offset: usize, pairs: usize },
}

impl fmt::Display for CorrespondenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CorrespondenceError::UnmappedLowTriangle { low } => {
                write!(f, "low triangle {low} maps to no high triangle")
            }
            CorrespondenceError::OffsetLengthMismatch { offsets, expected } => {
                write!(f, "offset array has {offsets} entries, expected {expected}")
            }
            CorrespondenceError::OffsetsNotMonotonic { low } => {
                write!(f, "offset array decreases at low triangle {low}")
            }
            CorrespondenceError::TrailingOffsetMismatch { offset, pairs } => {
                write!(
                    f,
                    "trailing offset is {offset} but the map holds {pairs} pairs"
                )
            }
        }
    }
}

impl std::error::Error for CorrespondenceError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn built(low_triangle_count: usize, pairs: &[(u32, u32)]) -> Correspondence {
        let mut builder = CorrespondenceBuilder::new(low_triangle_count);
        for &(low, high) in pairs {
            builder.push(low, high);
        }
        builder.build()
    }

    #[test]
    fn every_pushed_high_is_readable_from_its_low() {
        let map = built(2, &[(0, 7), (0, 9), (1, 4)]);
        assert_eq!(map.high_for(0), &[7, 9]);
        assert_eq!(map.high_for(1), &[4]);
    }

    #[test]
    fn build_is_independent_of_push_order() {
        let forward = built(2, &[(0, 7), (0, 9), (1, 4)]);
        let shuffled = built(2, &[(1, 4), (0, 9), (0, 7)]);
        assert_eq!(forward, shuffled);
    }

    #[test]
    fn a_repeated_pair_is_stored_once() {
        let map = built(1, &[(0, 3), (0, 3), (0, 3)]);
        assert_eq!(map.high_for(0), &[3]);
        assert_eq!(map.pair_count(), 1);
    }

    #[test]
    fn one_high_triangle_can_serve_several_low_triangles() {
        let map = built(3, &[(0, 5), (1, 5), (2, 5)]);
        assert_eq!(map.high_for(0), &[5]);
        assert_eq!(map.high_for(1), &[5]);
        assert_eq!(map.high_for(2), &[5]);
        assert_eq!(map.pair_count(), 3);
    }

    #[test]
    fn pair_count_counts_pairs_not_low_triangles() {
        let map = built(2, &[(0, 1), (0, 2), (0, 3), (1, 4)]);
        assert_eq!(map.low_triangle_count(), 2);
        assert_eq!(map.pair_count(), 4);
    }

    #[test]
    fn a_low_triangle_beyond_the_count_reads_as_empty() {
        let map = built(1, &[(0, 0)]);
        assert_eq!(map.high_for(9), &[] as &[u32]);
    }

    #[test]
    fn validate_accepts_a_fully_mapped_correspondence() {
        let map = built(3, &[(0, 0), (1, 1), (2, 2)]);
        assert_eq!(map.validate(), Ok(()));
    }

    #[test]
    fn validate_rejects_a_low_triangle_that_maps_to_nothing() {
        let map = built(3, &[(0, 0), (2, 2)]);
        assert_eq!(
            map.validate(),
            Err(CorrespondenceError::UnmappedLowTriangle { low: 1 })
        );
    }

    #[test]
    fn validate_rejects_an_empty_map_over_a_non_empty_low_mesh() {
        let map = built(2, &[]);
        assert!(map.is_empty());
        assert_eq!(
            map.validate(),
            Err(CorrespondenceError::UnmappedLowTriangle { low: 0 })
        );
    }

    #[test]
    fn an_empty_low_mesh_validates() {
        let map = built(0, &[]);
        assert_eq!(map.validate(), Ok(()));
    }

    #[test]
    fn errors_display_without_debug_formatting() {
        let error = CorrespondenceError::UnmappedLowTriangle { low: 12 };
        assert_eq!(
            error.to_string(),
            "low triangle 12 maps to no high triangle"
        );
    }
}
