//! 曲順の規則（シャッフル・リピート・前後の曲）。状態を持たない関数だけを置く。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

impl Repeat {
    pub fn parse(s: &str) -> Self {
        match s {
            "all" => Repeat::All,
            "one" => Repeat::One,
            _ => Repeat::Off,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Repeat::Off => "off",
            Repeat::All => "all",
            Repeat::One => "one",
        }
    }
    /// 押すたびに 切 → 全曲 → 1曲 → 切
    pub fn cycle(self) -> Self {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }
}

/// 小さな乱数（xorshift64*）。シャッフルにだけ使う。
pub struct Rng(u64);

impl Rng {
    pub fn seeded(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    pub fn from_time() -> Self {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Rng::seeded(t ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// 再生の順番（context の添字の並び）を作る。
/// シャッフルのときは `first` を先頭に置き、残りを混ぜる。
pub fn build_order(len: usize, shuffle: bool, first: Option<usize>, rng: &mut Rng) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    if shuffle && len > 1 {
        // Fisher–Yates
        for i in (1..len).rev() {
            let j = rng.below(i + 1);
            order.swap(i, j);
        }
        if let Some(f) = first.filter(|f| *f < len) {
            if let Some(p) = order.iter().position(|x| *x == f) {
                order.swap(0, p);
            }
        }
    }
    order
}

/// 次の位置。None なら終わり（リピートなしで最後まで来た）
pub fn next_pos(pos: usize, len: usize, repeat: Repeat) -> Option<usize> {
    if len == 0 {
        return None;
    }
    if pos + 1 < len {
        Some(pos + 1)
    } else if repeat == Repeat::All {
        Some(0)
    } else {
        None
    }
}

/// 前の位置。None なら先頭より前（曲の頭へ戻すだけ）
pub fn prev_pos(pos: usize, len: usize, repeat: Repeat) -> Option<usize> {
    if len == 0 {
        return None;
    }
    if pos > 0 {
        Some(pos - 1)
    } else if repeat == Repeat::All {
        Some(len - 1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_without_shuffle_is_identity() {
        let mut r = Rng::seeded(1);
        assert_eq!(build_order(4, false, Some(2), &mut r), vec![0, 1, 2, 3]);
    }

    #[test]
    fn shuffle_is_permutation_with_first() {
        let mut r = Rng::seeded(42);
        for first in 0..10 {
            let o = build_order(10, true, Some(first), &mut r);
            assert_eq!(o[0], first);
            let mut s = o.clone();
            s.sort();
            assert_eq!(s, (0..10).collect::<Vec<_>>());
        }
    }

    #[test]
    fn shuffle_actually_mixes() {
        let mut r = Rng::seeded(7);
        let o = build_order(50, true, None, &mut r);
        assert_ne!(o, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn next_and_prev_with_repeat() {
        assert_eq!(next_pos(0, 3, Repeat::Off), Some(1));
        assert_eq!(next_pos(2, 3, Repeat::Off), None);
        assert_eq!(next_pos(2, 3, Repeat::All), Some(0));
        assert_eq!(prev_pos(0, 3, Repeat::Off), None);
        assert_eq!(prev_pos(0, 3, Repeat::All), Some(2));
        assert_eq!(next_pos(0, 0, Repeat::All), None);
    }

    #[test]
    fn repeat_cycle() {
        assert_eq!(Repeat::Off.cycle(), Repeat::All);
        assert_eq!(Repeat::All.cycle(), Repeat::One);
        assert_eq!(Repeat::One.cycle(), Repeat::Off);
    }
}
