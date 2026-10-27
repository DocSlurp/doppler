//! The sub-quadratic lookups used by layout and tables agree with the scans they replace.

use doppler_md::layout::{near_any, near_other};
use doppler_md::pyc::fcmp;

/// Small deterministic generator (no dependency) with values that stress float edge cases.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn value(&mut self) -> f64 {
        match self.next() % 12 {
            0 => 0.0,
            1 => -0.0,
            2 => f64::INFINITY,
            3 => f64::NEG_INFINITY,
            4 => f64::MAX,
            5 => 1.5,
            _ => (self.next() % 400) as f64 / 8.0 - 10.0,
        }
    }
}

#[test]
fn near_any_matches_scan() {
    let mut g = Lcg(7);
    for _ in 0..20000 {
        let n = (g.next() % 9) as usize;
        let mut v: Vec<f64> = (0..n).map(|_| g.value()).collect();
        v.sort_by(|a, b| fcmp(*a, *b));
        let x = g.value();
        let tol = [0.0, 0.5, 1.5, 4.5, f64::INFINITY][(g.next() % 5) as usize];
        let want = v.iter().any(|&b| (x - b).abs() <= tol);
        assert_eq!(near_any(&v, x, tol), want, "{v:?} {x} {tol}");
    }
}

#[test]
fn near_other_matches_scan() {
    let mut g = Lcg(11);
    for _ in 0..20000 {
        let n = 1 + (g.next() % 9) as usize;
        let v: Vec<f64> = (0..n).map(|_| g.value()).collect();
        let me = (g.next() % n as u64) as usize;
        let tol = [0.5, 1.5, 4.5, f64::INFINITY][(g.next() % 4) as usize];
        let want = v.iter().enumerate().any(|(j, &b)| j != me && (b - v[me]).abs() <= tol);
        let mut s = v.clone();
        s.sort_by(|a, b| fcmp(*a, *b));
        assert_eq!(near_other(&s, v[me], tol), want, "{v:?} {me} {tol}");
    }
}
