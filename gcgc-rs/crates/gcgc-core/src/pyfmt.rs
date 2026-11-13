//! Python-compatible number formatting.
//!
//! The original tool builds labels and ASCII tables with `str(float)`, `round(x, n)` and
//! `"%9.4f" % x`. Rust's `Display` for `f64` differs (`100` vs `100.0`, never uses exponents),
//! so these helpers reproduce CPython's output exactly.

/// `repr(x)` / `str(x)` for a Python float (CPython "short" repr).
pub fn repr_f64(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    let (digits, decpt) = shortest_digits(x.abs());
    let n = digits.len() as i32;
    let body = if decpt <= -4 || decpt > 16 {
        let exp = decpt - 1;
        let mantissa = if n == 1 { digits.clone() } else { format!("{}.{}", &digits[..1], &digits[1..]) };
        let esign = if exp < 0 { '-' } else { '+' };
        format!("{mantissa}e{esign}{:02}", exp.abs())
    } else if decpt <= 0 {
        format!("0.{}{}", "0".repeat((-decpt) as usize), digits)
    } else if decpt < n {
        format!("{}.{}", &digits[..decpt as usize], &digits[decpt as usize..])
    } else {
        format!("{}{}.0", digits, "0".repeat((decpt - n) as usize))
    };
    format!("{sign}{body}")
}

/// Shortest round-trip decimal digits of a finite, non-negative `x`, and the decimal point
/// position: `x = 0.DIGITS * 10^decpt`.
fn shortest_digits(x: f64) -> (String, i32) {
    if x == 0.0 {
        return ("0".into(), 1);
    }
    let s = format!("{x:e}"); // e.g. "1.2345e-5", "1e16"
    let (mant, exp) = s.split_once('e').expect("LowerExp always has an exponent");
    let exp: i32 = exp.parse().expect("valid exponent");
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    (digits, exp + 1)
}

/// Python `round(x, ndigits)` for floats: correctly rounded, ties to even on the exact binary value.
pub fn round(x: f64, ndigits: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    format!("{x:.ndigits$}").parse().expect("formatted float parses")
}

/// Python `"%9.4f" % x` (and other widths/precisions): right-aligned fixed notation.
pub fn percent_f(x: f64, width: usize, precision: usize) -> String {
    let body = if x.is_nan() {
        "nan".to_string()
    } else if x.is_infinite() {
        if x > 0.0 {
            "inf".to_string()
        } else {
            "-inf".to_string()
        }
    } else {
        format!("{x:.precision$}")
    };
    format!("{body:>width$}")
}

/// Python `str()` of a number that may be integral. Values that came from Python ints in the
/// original (e.g. default percentiles `[50, 95, 99, 99.99]`) print without a trailing `.0`.
pub fn repr_num_intlike(x: f64) -> String {
    if x.is_finite() && x.fract() == 0.0 && x.abs() < 1e16 {
        format!("{}", x as i64)
    } else {
        repr_f64(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repr_matches_cpython() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (100.0, "100.0"),
            (3157.044, "3157.044"),
            (2.0670000000000006, "2.0670000000000006"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1e-5, "1e-05"),
            (0.0001, "0.0001"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (123456789012345680.0, "1.2345678901234568e+17"),
            (1.5e-7, "1.5e-07"),
            (f64::NAN, "nan"),
            (f64::INFINITY, "inf"),
            (-2.5, "-2.5"),
        ];
        for (x, want) in cases {
            assert_eq!(repr_f64(*x), *want, "repr({x:?})");
        }
    }

    #[test]
    fn round_matches_cpython() {
        assert_eq!(round(2.5, 0), 2.0);
        assert_eq!(round(0.125, 2), 0.12);
        assert_eq!(round(0.375, 2), 0.38);
        assert_eq!(repr_f64(round(3157.0440000000003, 4)), "3157.044");
        assert_eq!(repr_f64(round(1.005, 2)), "1.0"); // 1.005 is really 1.00499999...
    }

    #[test]
    fn percent_f_matches_cpython() {
        assert_eq!(percent_f(122.0, 9, 4), " 122.0000");
        assert_eq!(percent_f(0.00725, 9, 4), "   0.0073"); // binary value is just above the tie
        assert_eq!(percent_f(12345678.0, 9, 4), "12345678.0000");
    }
}
