//! Exact emulation of pandas `sort_values(kind="quicksort")` on a numeric column.
//!
//! pandas calls `nargsort`, which argsorts the non-null values with numpy's introsort
//! (`aquicksort`: median-of-3 quicksort, insertion sort for partitions of <= 16 elements,
//! heapsort fallback) and appends the null rows in their original order. The sort is *not*
//! stable, and the original notebook's later plots depend on the resulting order of rows with
//! equal timestamps, so we reproduce it exactly rather than using a stable sort.

const SMALL_QUICKSORT: usize = 15;

fn msb(mut n: usize) -> i32 {
    let mut depth = 0;
    while n > 1 {
        n >>= 1;
        depth += 1;
    }
    depth
}

/// numpy `aquicksort_` for doubles: returns `tosort` permuted so `v[tosort]` is ascending.
fn aquicksort(v: &[f64], tosort: &mut [usize]) {
    let num = tosort.len();
    if num < 2 {
        return;
    }
    let less = |a: f64, b: f64| a < b;
    let mut stack: Vec<(usize, usize, i32)> = Vec::new();
    let mut pl = 0usize;
    let mut pr = num - 1;
    let mut cdepth = msb(num) * 2;
    loop {
        let mut skip_to_pop = false;
        if cdepth < 0 {
            aheapsort(v, &mut tosort[pl..=pr]);
            skip_to_pop = true;
        }
        if !skip_to_pop {
            while pr - pl > SMALL_QUICKSORT {
                let pm = pl + ((pr - pl) >> 1);
                if less(v[tosort[pm]], v[tosort[pl]]) {
                    tosort.swap(pm, pl);
                }
                if less(v[tosort[pr]], v[tosort[pm]]) {
                    tosort.swap(pr, pm);
                }
                if less(v[tosort[pm]], v[tosort[pl]]) {
                    tosort.swap(pm, pl);
                }
                let vp = v[tosort[pm]];
                let mut pi = pl;
                let mut pj = pr - 1;
                tosort.swap(pm, pj);
                loop {
                    loop {
                        pi += 1;
                        if !less(v[tosort[pi]], vp) {
                            break;
                        }
                    }
                    loop {
                        pj -= 1;
                        if !less(vp, v[tosort[pj]]) {
                            break;
                        }
                    }
                    if pi >= pj {
                        break;
                    }
                    tosort.swap(pi, pj);
                }
                let pk = pr - 1;
                tosort.swap(pi, pk);
                cdepth -= 1;
                // push the larger partition, continue with the smaller one
                if pi - pl < pr - pi {
                    stack.push((pi + 1, pr, cdepth));
                    pr = pi - 1;
                } else {
                    stack.push((pl, pi - 1, cdepth));
                    pl = pi + 1;
                }
            }
            // insertion sort
            let mut pi = pl + 1;
            while pi <= pr {
                let vi = tosort[pi];
                let vp = v[vi];
                let mut pj = pi;
                while pj > pl && less(vp, v[tosort[pj - 1]]) {
                    tosort[pj] = tosort[pj - 1];
                    pj -= 1;
                }
                tosort[pj] = vi;
                pi += 1;
            }
        }
        match stack.pop() {
            None => break,
            Some((l, r, d)) => {
                pl = l;
                pr = r;
                cdepth = d;
            }
        }
    }
}

/// numpy `aheapsort_` (1-based heap over the index slice).
fn aheapsort(v: &[f64], tosort: &mut [usize]) {
    let n = tosort.len();
    if n < 2 {
        return;
    }
    // a[1..=n] maps to tosort[0..n]
    let get = |t: &[usize], i: usize| t[i - 1];
    let mut l = n >> 1;
    while l > 0 {
        let tmp = get(tosort, l);
        let mut i = l;
        let mut j = l << 1;
        while j <= n {
            if j < n && v[get(tosort, j)] < v[get(tosort, j + 1)] {
                j += 1;
            }
            if v[tmp] < v[get(tosort, j)] {
                tosort[i - 1] = tosort[j - 1];
                i = j;
                j += j;
            } else {
                break;
            }
        }
        tosort[i - 1] = tmp;
        l -= 1;
    }
    let mut n = n;
    while n > 1 {
        let tmp = get(tosort, n);
        tosort[n - 1] = tosort[0];
        n -= 1;
        let mut i = 1;
        let mut j = 2;
        while j <= n {
            if j < n && v[get(tosort, j)] < v[get(tosort, j + 1)] {
                j += 1;
            }
            if v[tmp] < v[get(tosort, j)] {
                tosort[i - 1] = tosort[j - 1];
                i = j;
                j += j;
            } else {
                break;
            }
        }
        tosort[i - 1] = tmp;
    }
}

/// Row order produced by pandas `sort_values(by=col)` (ascending, quicksort, nulls last).
pub fn pandas_sort_order(values: &[Option<f64>]) -> Vec<usize> {
    let non_null_idx: Vec<usize> = (0..values.len()).filter(|&i| values[i].is_some()).collect();
    let non_null: Vec<f64> = non_null_idx.iter().map(|&i| values[i].unwrap()).collect();
    let mut order: Vec<usize> = (0..non_null.len()).collect();
    aquicksort(&non_null, &mut order);
    let mut out: Vec<usize> = order.into_iter().map(|k| non_null_idx[k]).collect();
    out.extend((0..values.len()).filter(|&i| values[i].is_none()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_ascending() {
        let v: Vec<Option<f64>> =
            [5.0, 3.0, 9.0, 1.0, 1.0, 7.0, 2.0, 8.0, 0.0, 4.0, 6.0, 3.0, 2.0, 5.0, 1.0, 9.0, 0.5, 4.5, 2.5, 7.5]
                .iter()
                .map(|&x| Some(x))
                .chain([None])
                .collect();
        let order = pandas_sort_order(&v);
        let sorted: Vec<f64> = order.iter().filter_map(|&i| v[i]).collect();
        assert!(sorted.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(*order.last().unwrap(), 20);
    }

    #[test]
    fn heapsort_sorts() {
        let v = [3.0, 1.0, 2.0, 5.0, 4.0, 0.0];
        let mut idx: Vec<usize> = (0..6).collect();
        aheapsort(&v, &mut idx);
        assert_eq!(idx, vec![5, 1, 2, 0, 4, 3]);
    }
}
