//! numpy-compatible statistics (numpy 1.24 semantics), so results match the original bit for bit
//! where practical: pairwise summation, `percentile(method="linear")`, population `std`, and
//! numpy's `round` (scale, round-half-even, unscale).

/// numpy's pairwise summation (`@TYPE@_pairwise_sum`, block size 128, unroll 8).
fn pairwise(a: &[f64]) -> f64 {
    let n = a.len();
    if n < 8 {
        let mut res = 0.0;
        for x in a {
            res += x;
        }
        res
    } else if n <= 128 {
        let mut r = [0.0f64; 8];
        r.copy_from_slice(&a[..8]);
        let mut i = 8;
        while i < n - (n % 8) {
            for j in 0..8 {
                r[j] += a[i + j];
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        while i < n {
            res += a[i];
            i += 1;
        }
        res
    } else {
        let mut n2 = n / 2;
        n2 -= n2 % 8;
        pairwise(&a[..n2]) + pairwise(&a[n2..])
    }
}

/// `np.sum` of a float64 array (identity 0.0 plus the pairwise sum).
pub fn np_sum(a: &[f64]) -> f64 {
    0.0 + pairwise(a)
}

/// `np.mean`. NaN for an empty input (numpy warns and returns nan).
pub fn mean(a: &[f64]) -> f64 {
    np_sum(a) / a.len() as f64
}

/// `np.std` with ddof = 0.
pub fn std(a: &[f64]) -> f64 {
    let m = mean(a);
    let sq: Vec<f64> = a.iter().map(|x| (x - m) * (x - m)).collect();
    (np_sum(&sq) / a.len() as f64).sqrt()
}

/// `np.percentile(a, p)` with the default "linear" method. NaN for an empty input
/// (numpy raises IndexError there; callers report that separately).
pub fn percentile(a: &[f64], p: f64) -> f64 {
    let n = a.len();
    if n == 0 {
        return f64::NAN;
    }
    if a.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    let mut sorted = a.to_vec();
    sorted.sort_by(|x, y| x.partial_cmp(y).expect("no NaN"));
    let q = p / 100.0;
    let virtual_index = (n as f64 - 1.0) * q;
    let (prev, next, prev_f) = if virtual_index >= (n - 1) as f64 {
        (n - 1, n - 1, -1.0)
    } else if virtual_index < 0.0 {
        (0, 0, 0.0)
    } else {
        let f = virtual_index.floor();
        (f as usize, f as usize + 1, f)
    };
    let gamma = virtual_index - prev_f;
    lerp(sorted[prev], sorted[next], gamma)
}

/// numpy's `_lerp`: `a + (b-a)*t`, but `b - (b-a)*(1-t)` when `t >= 0.5`.
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    let diff = b - a;
    if t >= 0.5 {
        b - diff * (1.0 - t)
    } else {
        a + diff * t
    }
}

/// `round(np.float64(x), n)`: numpy rounds by scaling, rint (half-even) and unscaling, which is
/// not the same as Python's correctly rounded `round(float, n)`.
pub fn np_round(x: f64, decimals: i32) -> f64 {
    let scale = 10f64.powi(decimals);
    (x * scale).round_ties_even() / scale
}

/// `np.rot90` of a row-major matrix (counter-clockwise by 90°).
pub fn rot90<T: Copy>(m: &[Vec<T>]) -> Vec<Vec<T>> {
    if m.is_empty() {
        return Vec::new();
    }
    let rows = m.len();
    let cols = m[0].len();
    (0..cols).rev().map(|c| (0..rows).map(|r| m[r][c]).collect()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_linear() {
        let a = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile(&a, 50.0), 2.5);
        assert_eq!(percentile(&a, 100.0), 4.0);
        assert_eq!(percentile(&a, 0.0), 1.0);
        assert_eq!(percentile(&[7.0], 99.0), 7.0);
    }

    #[test]
    fn rot90_ccw() {
        let m = vec![vec![1, 2, 3], vec![4, 5, 6]];
        assert_eq!(rot90(&m), vec![vec![3, 6], vec![2, 5], vec![1, 4]]);
    }

    #[test]
    fn std_population() {
        let a = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        assert_eq!(std(&a), 2.0);
    }
}
