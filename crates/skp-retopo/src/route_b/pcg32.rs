const DEFAULT_STATE: u64 = 0x853c_49e6_748f_ea9b;
const DEFAULT_STREAM: u64 = 0xda3e_39cb_94b9_5bdb;
const MULT: u64 = 0x5851_f42d_4c95_7f2d;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Default for Pcg32 {
    fn default() -> Pcg32 {
        Pcg32 {
            state: DEFAULT_STATE,
            inc: DEFAULT_STREAM,
        }
    }
}

impl Pcg32 {
    pub fn seeded(init_state: u64, init_seq: u64) -> Pcg32 {
        let mut rng = Pcg32 {
            state: 0,
            inc: (init_seq << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(init_state);
        rng.next_u32();
        rng
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULT).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    pub fn next_u32_below(&mut self, bound: u32) -> u32 {
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let r = self.next_u32();
            if r >= threshold {
                return r % bound;
            }
        }
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.next_u32_below(i as u32 + 1) as usize;
            items.swap(i, j);
        }
    }

    pub fn next_f64(&mut self) -> f64 {
        let bits = ((self.next_u32() as u64) << 20) | 0x3ff0_0000_0000_0000;
        f64::from_bits(bits) - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_stream_matches_upstream() {
        let mut rng = Pcg32::default();
        let got: Vec<u32> = (0..4).map(|_| rng.next_u32()).collect();
        assert_eq!(got, [355248013, 41705475, 3406281715, 4186697710]);
    }

    #[test]
    fn seeded_stream_matches_the_pcg_reference_demo() {
        let mut rng = Pcg32::seeded(42, 54);
        let got: Vec<u32> = (0..6).map(|_| rng.next_u32()).collect();
        assert_eq!(
            got,
            [0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e]
        );
    }

    #[test]
    fn bounded_draws_match_upstream() {
        let mut rng = Pcg32::default();
        let got: Vec<u32> = (0..4).map(|_| rng.next_u32_below(10)).collect();
        assert_eq!(got, [3, 5, 5, 0]);
    }

    #[test]
    fn shuffle_draws_one_bounded_index_per_position_from_the_end() {
        let mut items = [0, 1, 2, 3, 4];
        Pcg32::default().shuffle(&mut items);
        let mut rng = Pcg32::default();
        let mut want = [0, 1, 2, 3, 4];
        for i in (1..5).rev() {
            let j = rng.next_u32_below(i as u32 + 1) as usize;
            want.swap(i, j);
        }
        assert_eq!(items, want);
        let mut sorted = items;
        sorted.sort();
        assert_eq!(sorted, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn doubles_match_upstream() {
        let mut rng = Pcg32::default();
        assert_eq!(rng.next_f64(), 355248013.0 / 4294967296.0);
        assert_eq!(rng.next_f64(), 41705475.0 / 4294967296.0);
    }
}
