//! A tiny deterministic RNG (PCG32, seeded with SplitMix64). Hand-rolled so results never
//! change between crate versions — the same seed always builds the same dungeon.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng {
    state: u64,
    inc: u64,
}

fn splitmix(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    pub fn new(seed: u64, stream: u64) -> Rng {
        let mut s = seed ^ stream.wrapping_mul(0xD1B5_4A32_D192_ED03);
        let mut r = Rng { state: 0, inc: (splitmix(&mut s) << 1) | 1 };
        r.state = splitmix(&mut s);
        r.next_u32();
        r
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in [lo, hi).
    pub fn range_f(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// Uniform integer in [lo, hi] (inclusive).
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        let span = (hi - lo + 1) as u32;
        lo + (self.next_u32() % span) as i32
    }

    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next_u32() as usize) % n }
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    pub fn pick<'a, T>(&mut self, v: &'a [T]) -> Option<&'a T> {
        if v.is_empty() { None } else { Some(&v[self.below(v.len())]) }
    }

    /// Index chosen with probability proportional to `weights`.
    pub fn pick_weighted(&mut self, weights: &[u32]) -> Option<usize> {
        let total: u64 = weights.iter().map(|&w| w as u64).sum();
        if total == 0 {
            return None;
        }
        let mut r = (self.next_u32() as u64 * total) >> 32;
        for (i, &w) in weights.iter().enumerate() {
            if r < w as u64 {
                return Some(i);
            }
            r -= w as u64;
        }
        Some(weights.len() - 1)
    }

    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let mut a = Rng::new(42, 1);
        let mut b = Rng::new(42, 1);
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
        let mut c = Rng::new(42, 2);
        assert_ne!(Rng::new(42, 1).next_u32(), c.next_u32());
    }

    #[test]
    fn ranges() {
        let mut r = Rng::new(7, 0);
        for _ in 0..1000 {
            let v = r.range(3, 5);
            assert!((3..=5).contains(&v));
            let f = r.f32();
            assert!((0.0..1.0).contains(&f));
        }
        assert_eq!(r.pick_weighted(&[0, 0, 5]), Some(2));
    }
}
