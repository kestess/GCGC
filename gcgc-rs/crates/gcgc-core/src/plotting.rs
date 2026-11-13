//! Ports of the plotting functions in `src/graphing/*.py`.
//!
//! Each function records on an [`Axes`] the calls the original made on a matplotlib axes, and
//! writes anything the original printed to [`Out`]. Where the original would raise, an `Err` with
//! a description is returned instead.

use crate::chart::{colors_and_alphas, Axes, Call, Color, Figure};
use crate::out::Out;
use crate::pyfmt::{self, repr_f64, repr_num_intlike};
use crate::query::{filter_and_group, Filter, Groups, Select};
use crate::stats;
use crate::table::Table;

pub type R<T> = Result<T, String>;

/// `round(x, n)` with Python or numpy semantics depending on where the value came from.
pub fn round_like(x: f64, n: i32, numpy: bool) -> f64 {
    if numpy {
        stats::np_round(x, n)
    } else {
        pyfmt::round(x, n as usize)
    }
}

/// Python `int(x)` (truncation toward zero).
fn py_int(x: f64) -> i64 {
    x.trunc() as i64
}

fn values(v: &[Option<f64>]) -> Vec<f64> {
    v.iter().flatten().copied().collect()
}

fn check_counts(out: &mut Out, series: usize, labels: usize, colors: usize) {
    if series > labels {
        out.println("Not enough labels to plot");
    }
    if series > colors {
        out.println("Not enough colors to plot");
    }
}

fn marker_plot(x: Vec<Option<f64>>, y: Vec<Option<f64>>, label: &str, color: &Color) -> Call {
    Call::Plot {
        x,
        y,
        label: Some(label.to_string()),
        color: Some(color.clone()),
        alpha: None,
        marker: Some("o".into()),
        linestyle: Some(String::new()),
        markersize: Some(3.0),
    }
}

fn line(x: Vec<Option<f64>>, y: Vec<Option<f64>>, label: Option<&str>, color: &Color, alpha: Option<f64>) -> Call {
    Call::Plot {
        x,
        y,
        label: label.map(str::to_string),
        color: Some(color.clone()),
        alpha,
        marker: None,
        linestyle: None,
        markersize: None,
    }
}

fn scatter(x: Vec<Option<f64>>, y: Vec<Option<f64>>, label: Option<&str>, color: &Color, alpha: Option<f64>) -> Call {
    Call::Scatter { x, y, label: label.map(str::to_string), color: Some(color.clone()), alpha }
}

fn line_or_scatter(
    line_graph: bool,
    x: Vec<Option<f64>>,
    y: Vec<Option<f64>>,
    label: Option<&str>,
    color: &Color,
    alpha: Option<f64>,
) -> Call {
    if line_graph {
        line(x, y, label, color, alpha)
    } else {
        scatter(x, y, label, color, alpha)
    }
}

fn some(v: Vec<f64>) -> Vec<Option<f64>> {
    v.into_iter().map(Some).collect()
}

/// `plot_scatter`: markers of `column` against time.
pub fn plot_scatter(ax: &mut Axes, tables: &[&Table], sel: &Select, out: &mut Out) {
    let g = filter_and_group(tables, sel, out);
    check_counts(out, g.y.len(), g.labels.len(), g.colors.len());
    for (((x, y), c), l) in g.x.into_iter().zip(g.y).zip(&g.colors).zip(&g.labels) {
        ax.push(marker_plot(x, y, l, c));
    }
    ax.legend = true;
}

/// `plot_line`
pub fn plot_line(ax: &mut Axes, tables: &[&Table], sel: &Select, out: &mut Out) {
    let g = filter_and_group(tables, sel, out);
    check_counts(out, g.y.len(), g.labels.len(), g.colors.len());
    for (((x, y), c), l) in g.x.into_iter().zip(g.y).zip(&g.colors).zip(&g.labels) {
        ax.push(line(x, y, Some(l), c, None));
    }
    ax.legend = true;
}

/// Python `sum(series)`: sequential addition starting from int 0.
fn py_sum(v: &[Option<f64>]) -> f64 {
    v.iter().flatten().fold(0.0, |a, b| a + b)
}

/// `plot_bar_sum`: one bar per group with the sum of `column`.
pub fn plot_bar_sum(ax: &mut Axes, tables: &[&Table], sel: &Select, out: &mut Out) {
    let g = filter_and_group(tables, sel, out);
    check_counts(out, g.y.len(), g.labels.len(), g.colors.len());
    for (idx, ((y, c), (l, a))) in g.y.iter().zip(&g.colors).zip(g.labels.iter().zip(&g.alphas)).enumerate() {
        let h = py_sum(y);
        let shown = if y.is_empty() { "0".to_string() } else { repr_f64(round_like(h, 4, g.numpy)) };
        ax.push(Call::Bar {
            x: idx as f64,
            height: h,
            width: None,
            label: Some(format!("{l} : {shown}")),
            color: Some(c.clone()),
            alpha: Some(*a),
        });
    }
    ax.xticks = Some((0..g.y.len()).map(|i| i as f64).collect());
    ax.xticklabels = Some(g.labels.clone());
    ax.legend = true;
}

/// `plot_bar_avg`: one bar per group with the mean of `column`.
pub fn plot_bar_avg(ax: &mut Axes, tables: &[&Table], sel: &Select, out: &mut Out) -> R<()> {
    let g = filter_and_group(tables, sel, out);
    check_counts(out, g.y.len(), g.labels.len(), g.colors.len());
    for (idx, ((y, c), l)) in g.y.iter().zip(&g.colors).zip(&g.labels).enumerate() {
        if y.is_empty() {
            return Err("ZeroDivisionError: division by zero (empty group in plot_bar_avg)".into());
        }
        let h = py_sum(y) / y.len() as f64;
        ax.push(Call::Bar {
            x: idx as f64,
            height: h,
            width: None,
            label: Some(format!("{l} : {}", repr_f64(round_like(h, 4, g.numpy)))),
            color: Some(c.clone()),
            alpha: None,
        });
    }
    ax.xticks = Some((0..g.y.len()).map(|i| i as f64).collect());
    ax.xticklabels = Some(g.labels.clone());
    ax.legend = true;
    Ok(())
}

/// `plot_reclaimed_bytes`: HeapBeforeGC − HeapAfterGC over time (scatter).
/// Note: like the original, legend labels come from the `labels` argument, not the group labels.
pub fn plot_reclaimed_bytes(ax: &mut Axes, tables: &[&Table], sel: &Select, out: &mut Out) -> R<()> {
    let before = filter_and_group(tables, &Select { column: "HeapBeforeGC", ..sel.clone() }, out);
    let colors = before.colors.clone();
    let after =
        filter_and_group(tables, &Select { column: "HeapAfterGC", colors: Some(colors.clone()), ..sel.clone() }, out);
    let labels = sel.labels.clone().ok_or("TypeError: object of type 'NoneType' has no len()")?;
    let reclaimed: Vec<Vec<Option<f64>>> = before
        .y
        .iter()
        .zip(&after.y)
        .map(|(b, a)| {
            b.iter()
                .zip(a)
                .map(|(b, a)| match (b, a) {
                    (Some(b), Some(a)) => Some(b - a),
                    _ => None,
                })
                .collect()
        })
        .collect();
    check_counts(out, after.x.len(), labels.len(), colors.len());
    for (((x, y), c), l) in after.x.into_iter().zip(reclaimed).zip(&colors).zip(&labels) {
        ax.push(scatter(x, y, Some(l), c, None));
    }
    ax.legend = true;
    Ok(())
}

/// `get_buckets_and_range`: (number of buckets, min, max) over all non-empty series.
fn buckets_and_range(groups: &[Vec<Option<f64>>], interval: f64) -> R<(usize, f64, f64)> {
    let first = groups.first().and_then(|g| g.first()).ok_or("IndexError: no data to bucket (empty selection)")?;
    let mut max = 0.0f64;
    let mut min = first.unwrap_or(f64::NAN);
    for g in groups {
        let v = values(g);
        if !g.is_empty() {
            let gmax = v.iter().copied().fold(f64::NAN, f64::max);
            let gmin = v.iter().copied().fold(f64::NAN, f64::min);
            // Python max(a, b) / min(a, b) keep `a` unless `b` is strictly greater / smaller.
            max = if max > gmax { max } else { gmax };
            min = if min < gmin { min } else { gmin };
        }
    }
    let n = if interval != 0.0 { (py_int((max - min) / interval) + 1).max(0) as usize } else { 0 };
    Ok((n, min, max))
}

/// `group_into_buckets`: data into `n` time buckets; empty buckets get a single 0.
fn group_into_buckets(times: &[Option<f64>], data: &[Option<f64>], n: usize, interval: f64) -> R<Vec<Vec<f64>>> {
    let mut buckets = vec![Vec::new(); n];
    for (t, d) in times.iter().zip(data) {
        let (Some(t), Some(d)) = (t, d) else { continue };
        let b = py_int(t / interval);
        if b < 0 || b as usize >= n {
            return Err(format!("IndexError: list index out of range (bucket {b} of {n})"));
        }
        buckets[b as usize].push(*d);
    }
    for b in buckets.iter_mut() {
        if b.is_empty() {
            b.push(0.0);
        }
    }
    Ok(buckets)
}

fn shifted(times: &[Option<f64>], min: f64) -> Vec<Option<f64>> {
    times.iter().map(|t| t.map(|t| t - min)).collect()
}

fn x_alignment(n: usize, interval: f64, min: f64) -> Vec<Option<f64>> {
    (0..n).map(|i| Some(i as f64 * interval + min)).collect()
}

/// `plot_frequency_intervals`: histogram of `column` values, grouped bars per series.
pub fn plot_frequency_intervals(ax: &mut Axes, tables: &[&Table], sel: &Select, interval: f64, out: &mut Out) -> R<()> {
    if interval == 0.0 {
        out.println("No interval length provided. Abort.");
        return Ok(());
    }
    let g = filter_and_group(tables, sel, out);
    if g.y.len() > g.colors.len() {
        out.println("Not enough colors to plot");
    }
    let (n, min, _) = buckets_and_range(&g.y, interval)?;
    let k = g.y.len();
    let mut counts = vec![vec![0u64; k]; n];
    for (idx, ds) in g.y.iter().enumerate() {
        for v in ds.iter().flatten() {
            let b = py_int((v - min) / interval);
            if b < 0 || b as usize >= n {
                return Err(format!("IndexError: list index out of range (bucket {b} of {n})"));
            }
            counts[b as usize][idx] += 1;
        }
    }
    let width = 0.8 / k as f64;
    for (bn, bucket) in counts.iter().enumerate() {
        for (idx, v) in bucket.iter().enumerate() {
            ax.push(Call::Bar {
                x: bn as f64 + width * idx as f64,
                height: *v as f64,
                width: Some(width),
                label: if bn == 0 { g.labels.get(idx).cloned() } else { None },
                color: Some(g.colors.get(idx).cloned().ok_or("IndexError: not enough colors")?),
                alpha: None,
            });
        }
    }
    let ticks: Vec<f64> = (0..n).map(|num| num as f64 + width / 2.0 * k as f64 - width / 2.0).collect();
    // An integral interval behaves like a Python int (labels "10", "20"); otherwise floats ("5.0").
    let int_interval = interval.fract() == 0.0;
    let labels: Vec<String> = (0..n)
        .map(|b| {
            let v = pyfmt::round(interval * (b + 1) as f64, 4);
            if int_interval {
                repr_num_intlike(v)
            } else {
                repr_f64(v)
            }
        })
        .collect();
    // simplify_xtickslabels(ticks, labels, 20)
    let (ticks, labels) = if ticks.len() < 20 {
        (ticks, labels)
    } else {
        let step = ticks.len().div_ceil(20);
        (ticks.into_iter().step_by(step).collect(), labels.into_iter().step_by(step).collect())
    };
    ax.xticks = Some(ticks);
    ax.xticklabels = Some(labels);
    ax.legend = true;
    Ok(())
}

/// `plot_percentile_intervals`: per time interval, the given percentiles of `column`.
#[allow(clippy::too_many_arguments)]
pub fn plot_percentile_intervals(
    ax: &mut Axes,
    tables: &[&Table],
    sel: &Select,
    interval: f64,
    percentiles: &[f64],
    line_graph: bool,
    different_colors: Option<bool>,
    out: &mut Out,
) -> R<()> {
    if interval == 0.0 {
        out.println("No interval length provided. Abort.");
        return Ok(());
    }
    let g = filter_and_group(tables, sel, out);
    let different = different_colors.unwrap_or(!line_graph);
    let colors = if different { colors_and_alphas(g.colors.len() * percentiles.len()).0 } else { g.colors.clone() };
    let (n, min, _) = buckets_and_range(&g.x, interval)?;
    let xs = x_alignment(n, interval, min);
    check_counts(out, g.x.len().max(g.y.len()), g.labels.len(), colors.len());
    for (group, (times, data)) in g.x.iter().zip(&g.y).enumerate() {
        let buckets = group_into_buckets(&shifted(times, min), data, n, interval)?;
        let per_bucket: Vec<Vec<f64>> =
            buckets.iter().map(|b| percentiles.iter().map(|p| stats::percentile(b, *p)).collect()).collect();
        let label = g.labels.get(group).ok_or("IndexError: not enough labels")?;
        let base = colors.get(group).ok_or("IndexError: not enough colors")?;
        let first: Vec<f64> = per_bucket.iter().map(|b| b[0]).collect();
        ax.push(line_or_scatter(line_graph, xs.clone(), some(first), Some(label), base, None));
        for idx in 1..percentiles.len() {
            let ys: Vec<f64> = per_bucket.iter().map(|b| b[idx]).collect();
            let color = if different { colors.get(group + idx).ok_or("IndexError: not enough colors")? } else { base };
            ax.push(line_or_scatter(line_graph, xs.clone(), some(ys), None, color, Some(1.0 - 0.15 * idx as f64)));
        }
    }
    ax.legend = true;
    Ok(())
}

/// `plot_frequency_of_gc_intervals`: number of distinct GC indices per interval (always a line).
pub fn plot_frequency_of_gc_intervals(
    ax: &mut Axes,
    tables: &[&Table],
    sel: &Select,
    interval: f64,
    out: &mut Out,
) -> R<()> {
    if interval == 0.0 {
        out.println("No interval length provided. Abort.");
        return Ok(());
    }
    let g = filter_and_group(tables, sel, out);
    let (n, min, _) = buckets_and_range(&g.x, interval)?;
    let xs = x_alignment(n, interval, min);
    check_counts(out, g.x.len().max(g.y.len()), g.labels.len(), g.colors.len());
    for (index, (times, data)) in g.x.iter().zip(&g.y).enumerate() {
        let buckets = group_into_buckets(&shifted(times, min), data, n, interval)?;
        // `len(set(bucket))`; note an empty interval holds [0] and so counts as 1, like the original.
        let freq: Vec<f64> = buckets
            .iter()
            .map(|b| {
                let mut v: Vec<u64> = b.iter().map(|x| if *x == 0.0 { 0 } else { x.to_bits() }).collect();
                v.sort_unstable();
                v.dedup();
                v.len() as f64
            })
            .collect();
        let label = g.labels.get(index).ok_or("IndexError: not enough labels")?;
        let color = g.colors.get(index).ok_or("IndexError: not enough colors")?;
        ax.push(line(xs.clone(), some(freq), Some(label), color, None));
    }
    ax.legend = true;
    Ok(())
}

/// `plot_using_intervals` / `plot_sum_pause_intervals`: sum of `column` per time interval.
pub fn plot_sum_intervals(
    ax: &mut Axes,
    tables: &[&Table],
    sel: &Select,
    interval: f64,
    remove_empty_intervals: bool,
    line_graph: bool,
    out: &mut Out,
) -> R<()> {
    if interval == 0.0 {
        out.println("No interval length provided. Abort.");
        return Ok(());
    }
    let g = filter_and_group(tables, sel, out);
    if g.x.is_empty() || g.x[0].is_empty() {
        return Ok(());
    }
    let (n, min, _) = buckets_and_range(&g.x, interval)?;
    let mut xs = x_alignment(n, interval, min);
    check_counts(out, g.x.len().max(g.y.len()), g.labels.len(), g.colors.len());
    for (index, (times, data)) in g.x.iter().zip(&g.y).enumerate() {
        let mut buckets = group_into_buckets(&shifted(times, min), data, n, interval)?;
        if remove_empty_intervals {
            let all = x_alignment(n, interval, min);
            let keep: Vec<usize> = (0..buckets.len()).filter(|&i| buckets[i].iter().sum::<f64>() != 0.0).collect();
            xs = keep.iter().map(|&i| all[i]).collect();
            buckets = keep.iter().map(|&i| buckets[i].clone()).collect();
        }
        let sums: Vec<f64> = buckets.iter().map(|b| b.iter().fold(0.0, |a, x| a + x)).collect();
        let label = g.labels.get(index).ok_or("IndexError: not enough labels")?;
        let color = g.colors.get(index).ok_or("IndexError: not enough colors")?;
        ax.push(line_or_scatter(line_graph, xs.clone(), some(sums), Some(label), color, None));
    }
    ax.legend = true;
    Ok(())
}

/// `calculate_allocation_rate`: (HeapBeforeGC[i+1] − HeapAfterGC[i]) / Δt, mean or percentile per
/// interval.
#[allow(clippy::too_many_arguments)]
pub fn plot_allocation_rate(
    ax: &mut Axes,
    tables: &[&Table],
    sel: &Select,
    interval: f64,
    percentile: Option<f64>,
    line_graph: bool,
    out: &mut Out,
) -> R<()> {
    let before = filter_and_group(tables, &Select { column: "HeapBeforeGC", ..sel.clone() }, out);
    // The original passes the first call's (group) labels and colors into the second call.
    let after = filter_and_group(
        tables,
        &Select {
            column: "HeapAfterGC",
            labels: Some(before.labels.clone()),
            colors: Some(before.colors.clone()),
            ..sel.clone()
        },
        out,
    );
    if before.y.len() > after.labels.len() {
        out.println("Not enough labels to plot");
    }
    if after.y.len() > after.colors.len() {
        out.println("Not enough colors to plot");
    }
    for ((((time, b), a), color), label) in
        after.x.iter().zip(&before.y).zip(&after.y).zip(&after.colors).zip(&after.labels)
    {
        let (xs, ys) = allocation_difference(time, b, a, interval, percentile)?;
        ax.push(line_or_scatter(line_graph, some(xs), some(ys), Some(label), color, None));
    }
    ax.legend = true;
    Ok(())
}

/// `get_difference`
fn allocation_difference(
    time: &[Option<f64>],
    before: &[Option<f64>],
    after: &[Option<f64>],
    interval: f64,
    percentile: Option<f64>,
) -> R<(Vec<f64>, Vec<f64>)> {
    let num = |v: &[Option<f64>], i: usize| v[i].ok_or_else(|| "TypeError: unsupported operand (None)".to_string());
    let mut times = Vec::new();
    let mut rates = Vec::new();
    let mut start = num(time, 0).map_err(|_| "IndexError: list index out of range".to_string())?;
    let mut window = Vec::new();
    for i in 0..before.len().saturating_sub(1) {
        let dt = num(time, i + 1)? - num(time, i)?;
        if dt != 0.0 {
            window.push((num(before, i + 1)? - num(after, i)?) / dt);
            let elapsed = num(time, i + 1)? - start;
            if elapsed >= interval {
                times.push(num(time, i)?);
                rates.push(match percentile {
                    None => stats::mean(&window),
                    Some(p) => stats::percentile(&window, p),
                });
                start = num(time, i + 1)?;
                window.clear();
            }
        }
    }
    Ok((times, rates))
}

/// `calculate_code_cache_flushing_rate`: flushes per second between flushing events.
pub fn plot_code_cache_flushing_rate(
    ax: &mut Axes,
    tables: &[&Table],
    labels: &[String],
    colors: Option<Vec<Color>>,
    line_graph: bool,
    out: &mut Out,
) -> R<()> {
    if tables.len() > labels.len() {
        out.println("Not enough labels to plot");
    }
    let mut series = Vec::new();
    for (i, t) in tables.iter().enumerate() {
        let (rates, ts) = flushing_rates(t)?;
        if !rates.is_empty() {
            series.push((rates, ts, labels.get(i).cloned().ok_or("IndexError: not enough labels")?));
        }
    }
    let colors = match colors {
        None => colors_and_alphas(series.len()).0,
        Some(c) => {
            if series.len() > c.len() {
                out.println("Not enough colors to plot");
            }
            c
        }
    };
    for ((rates, ts, label), color) in series.into_iter().zip(&colors) {
        ax.push(line_or_scatter(line_graph, some(ts), some(rates), Some(&label), color, None));
    }
    ax.legend = true;
    Ok(())
}

/// `get_rates_with_timestamps`
fn flushing_rates(t: &Table) -> R<(Vec<f64>, Vec<f64>)> {
    let flush = t.column("CodeCacheFlushing").ok_or("KeyError: 'CodeCacheFlushing'")?;
    let time = t.num("TimeFromStart_seconds").ok_or("KeyError: 'TimeFromStart_seconds'")?;
    let (mut rates, mut stamps) = (Vec::new(), Vec::new());
    let mut flushes = 0u64;
    let mut previous = 0.0;
    for (r, now) in time.iter().enumerate() {
        if !flush.get(r).is_null() {
            flushes += 1;
            let now = now.ok_or("TypeError: unsupported operand (None)")?;
            let dt = now - previous;
            if dt > 0.0 {
                rates.push(flushes as f64 / dt);
                stamps.push(now);
                flushes = 0;
                previous = now;
            }
        }
    }
    Ok((rates, stamps))
}

/// Grouping functions for [`plot_scatter_universal`] (the original took any Python callable).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "fn", rename_all = "snake_case")]
pub enum GroupingFn {
    Sum,
    Mean,
    Max,
    Min,
    Count,
    Percentile {
        p: f64,
    },
    /// `x / total * 100` (e.g. heap MB as a percentage of an 8 GB heap: `total = 8192`).
    PercentOf {
        total: f64,
    },
}

impl GroupingFn {
    fn apply(&self, v: &[f64]) -> f64 {
        match self {
            GroupingFn::Sum => v.iter().fold(0.0, |a, x| a + x),
            GroupingFn::Mean => stats::mean(v),
            GroupingFn::Max => v.iter().copied().fold(f64::NAN, f64::max),
            GroupingFn::Min => v.iter().copied().fold(f64::NAN, f64::min),
            GroupingFn::Count => v.len() as f64,
            GroupingFn::Percentile { p } => stats::percentile(v, *p),
            GroupingFn::PercentOf { total } => v.iter().fold(0.0, |a, x| a + x) / total * 100.0,
        }
    }
}

/// `plot_scatter_universal` (without the broken `include_timing_in_bucket` mode).
#[allow(clippy::too_many_arguments)]
pub fn plot_scatter_universal(
    ax: &mut Axes,
    tables: &[&Table],
    sel: &Select,
    interval: Option<f64>,
    grouping: GroupingFn,
    line_graph: bool,
    out: &mut Out,
) -> R<()> {
    let g: Groups = filter_and_group(tables, sel, out);
    check_counts(out, g.y.len(), g.labels.len(), g.colors.len());
    match interval {
        Some(interval) if interval != 0.0 => {
            // determine_extremes: first/last timestamps (assumes sorted series)
            let first = g.x.first().and_then(|x| x.first()).copied().flatten().ok_or("IndexError: no data")?;
            let (mut min, mut max) = (first, first);
            for x in &g.x {
                if let (Some(Some(a)), Some(Some(b))) = (x.first(), x.last()) {
                    min = min.min(*a);
                    max = max.max(*b);
                }
            }
            let count = (py_int((max - min) / interval) + 1).max(0) as usize;
            for (((x, y), label), color) in g.x.iter().zip(&g.y).zip(&g.labels).zip(&g.colors) {
                out.println("Before population");
                let mut buckets = vec![Vec::new(); count];
                for (t, d) in x.iter().zip(y) {
                    let (Some(t), Some(d)) = (t, d) else { continue };
                    let b = py_int((t - min) / interval);
                    if b < 0 || b as usize >= count {
                        return Err(format!("IndexError: list index out of range (bucket {b} of {count})"));
                    }
                    buckets[b as usize].push(*d);
                }
                for b in buckets.iter_mut() {
                    if b.is_empty() {
                        b.push(0.0);
                    }
                }
                let xs: Vec<f64> = (0..count).map(|i| py_int(min + interval * i as f64 + 1.0) as f64).collect();
                out.println("After population");
                let ys: Vec<f64> = buckets.iter().map(|b| grouping.apply(b)).collect();
                ax.push(line_or_scatter(line_graph, some(xs), some(ys), Some(label), color, None));
            }
        }
        _ => {
            for (((x, y), label), color) in g.x.iter().zip(&g.y).zip(&g.labels).zip(&g.colors) {
                let ys: Vec<Option<f64>> = y.iter().map(|v| v.map(|v| grouping.apply(&[v]))).collect();
                ax.push(line_or_scatter(line_graph, x.clone(), ys, Some(label), color, None));
            }
        }
    }
    ax.legend = true;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Heatmaps (heatmap.py)
// ---------------------------------------------------------------------------------------------

/// Python `a % b` for floats (result has the sign of `b`).
fn py_mod(a: f64, b: f64) -> f64 {
    let m = a % b;
    if m != 0.0 && ((b < 0.0) != (m < 0.0)) {
        m + b
    } else if m == 0.0 {
        0.0f64.copysign(b)
    } else {
        m
    }
}

/// Linear heatmap dimensions: `[x buckets, y buckets, x bucket seconds, y bucket milliseconds]`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HeatmapDims {
    pub x_buckets: usize,
    pub y_buckets: usize,
    pub x_seconds: f64,
    pub y_size: f64,
}

/// `plot_heatmaps`: one linear latency heatmap figure per series.
pub fn plot_heatmaps(
    tables: &[&Table],
    sel: &Select,
    dims: HeatmapDims,
    annotate: bool,
    out: &mut Out,
) -> R<Vec<Figure>> {
    let g = filter_and_group(tables, &Select { colors: None, ..sel.clone() }, out);
    let HeatmapDims { x_buckets: xc, y_buckets: yc, x_seconds: xd, y_size: yd } = dims;
    if xc == 0 || yc == 0 || xd <= 0.0 || yd <= 0.0 {
        out.println("Warning: All dimensions must be greater than zero.");
        return Ok(Vec::new());
    }
    if g.x.is_empty() {
        out.println("Fatal error: No timestamps collected. Abort");
        return Err("TypeError: no timestamps collected".into());
    }
    // __get_minimum_time
    let mut min_time = g.x[0][0].unwrap_or(f64::NAN);
    for x in &g.x {
        let m = values(x).into_iter().fold(f64::NAN, f64::min);
        min_time = if min_time < m { min_time } else { m };
    }
    let min_time = py_int(min_time - py_mod(min_time, xd)) as f64;

    let mut maps = Vec::new();
    for ((times, pauses), label) in g.x.iter().zip(&g.y).zip(&g.labels) {
        if times.is_empty() || pauses.is_empty() {
            continue;
        }
        let mut xb: Vec<Vec<Option<f64>>> = vec![Vec::new(); xc];
        let mut out_of_range_time = 0;
        for (p, t) in pauses.iter().zip(times) {
            let t = t.ok_or("TypeError: unsupported operand (None)")?;
            let b = py_int((t - min_time) / xd);
            if b == xc as i64 {
                xb[xc - 1].push(*p);
            } else if b < xc as i64 {
                xb[b.max(0) as usize].push(*p);
            } else {
                out_of_range_time += 1;
            }
        }
        let mut max_value = 0.0f64;
        for p in pauses.iter().flatten() {
            if *p != 0.0 && *p > max_value {
                max_value = *p;
            }
        }
        let mut grid = Vec::new();
        let mut out_of_range_latency = 0;
        for bucket in &xb {
            let mut yb = vec![0.0; yc];
            for v in bucket.iter().flatten() {
                if *v != 0.0 {
                    let b = py_int(v / yd);
                    if b < yc as i64 {
                        yb[b.max(0) as usize] += 1.0;
                    } else if b == yc as i64 {
                        yb[yc - 1] += 1.0;
                    } else {
                        out_of_range_latency += 1;
                    }
                }
            }
            grid.push(yb);
        }
        let grid = stats::rot90(&grid);
        if out_of_range_time > 0 {
            let max_t = values(times).into_iter().fold(f64::NAN, f64::max);
            out.println(&format!(
                "{label} Warning: {out_of_range_time} values lies outside of the provided time range. Max value outside range: {}",
                repr_f64(max_t)
            ));
        }
        if out_of_range_latency > 0 {
            out.println(&format!(
                "{label} Warning: {out_of_range_latency} values lies outside the provided range for latency. Max value outside range: {}",
                repr_f64(max_value)
            ));
        }
        maps.push(grid);
    }

    // plot_heatmap labels
    let mult = (xc as f64 * xd) / xc as f64;
    let time_labels: Vec<String> = (1..=xc)
        .enumerate()
        .map(|(i, num)| {
            if i % 2 == 0 {
                format!("{} s", repr_f64(pyfmt::round(num as f64 * mult + min_time, 2)))
            } else {
                String::new()
            }
        })
        .collect();
    let ymult = (yc as f64 * yd) / yc as f64;
    let latency_labels: Vec<String> =
        (1..=yc).rev().map(|num| format!("{} ms", repr_f64(pyfmt::round(num as f64 * ymult, 2)))).collect();

    let mut figs = Vec::new();
    for (grid, label) in maps.into_iter().zip(&g.labels) {
        figs.push(heatmap_figure(
            grid,
            &latency_labels,
            &time_labels,
            annotate,
            &format!("Latency during runtime: {label}"),
            out,
        ));
    }
    Ok(figs)
}

/// Logarithmic heatmap dimensions: `[x buckets, y buckets, x bucket seconds, base]`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LogHeatmapDims {
    pub x_buckets: usize,
    pub y_buckets: usize,
    pub x_seconds: f64,
    pub base: f64,
}

/// `plot_heatmaps_logarithmic`: one heatmap per series with logarithmic latency buckets.
pub fn plot_heatmaps_logarithmic(
    tables: &[&Table],
    sel: &Select,
    dims: LogHeatmapDims,
    annotate: bool,
    out: &mut Out,
) -> R<Vec<Figure>> {
    let g = filter_and_group(tables, sel, out);
    let LogHeatmapDims { x_buckets: xc, y_buckets: yc, x_seconds: xd, base } = dims;
    if g.y.is_empty() {
        return Err("ValueError: max() arg is an empty sequence".into());
    }
    let mut max_number = f64::NAN;
    for y in &g.y {
        let m = values(y).into_iter().fold(f64::NAN, f64::max);
        if m.is_nan() {
            return Err("ValueError: max() arg is an empty sequence".into());
        }
        max_number = if max_number > m || m.is_nan() { max_number } else { m };
    }
    // get_bucket_upper_ranges
    let mut ranges = vec![max_number];
    for _ in 0..yc.saturating_sub(1) {
        let last = *ranges.last().unwrap();
        ranges.push(last / base);
    }
    ranges.reverse();

    let mut maps = Vec::new();
    for (times, pauses) in g.x.iter().zip(&g.y) {
        let mut xb: Vec<Vec<f64>> = vec![Vec::new(); xc];
        let mut out_of_range_time = 0;
        for (p, t) in pauses.iter().zip(times) {
            let (Some(p), Some(t)) = (p, t) else { return Err("TypeError: unsupported operand (None)".into()) };
            let b = py_int(t / xd);
            if b == xc as i64 {
                xb[xc - 1].push(*p);
            } else if b < xc as i64 {
                xb[b.max(0) as usize].push(*p);
            } else {
                out_of_range_time += 1;
            }
        }
        let mut grid = Vec::new();
        let mut out_of_range_latency = 0;
        for bucket in &xb {
            let mut yb = vec![0.0; yc];
            for p in bucket {
                match binary_search(&ranges, *p) {
                    None => out_of_range_latency += 1,
                    Some(i) => yb[i] += 1.0,
                }
            }
            grid.push(yb);
        }
        let grid = stats::rot90(&grid);
        if out_of_range_time > 0 {
            let max_t = values(times).into_iter().fold(f64::NAN, f64::max);
            out.println(&format!(
                " Warning: {out_of_range_time} values lies outside of the provided time range. Max value outside range: {}",
                repr_f64(max_t)
            ));
        }
        if out_of_range_latency > 0 {
            out.println(&format!(
                " Warning: {out_of_range_latency} values lies outside the provided range for latency. Max value outside range: {}",
                repr_f64(max_number)
            ));
        }
        maps.push(grid);
    }

    let mult = (xc as f64 * xd) / xc as f64;
    let time_labels: Vec<String> = (1..=xc)
        .enumerate()
        .map(
            |(i, num)| {
                if i % 2 == 0 {
                    format!("{} s", repr_f64(pyfmt::round(num as f64 * mult, 2)))
                } else {
                    String::new()
                }
            },
        )
        .collect();
    let latency_labels: Vec<String> = ranges.iter().rev().map(|r| repr_f64(round_like(*r, 4, g.numpy))).collect();

    let mut figs = Vec::new();
    for (grid, label) in maps.into_iter().zip(&g.labels) {
        figs.push(heatmap_figure(
            grid,
            &latency_labels,
            &time_labels,
            annotate,
            &format!("Latency during runtime: {label}"),
            out,
        ));
    }
    Ok(figs)
}

/// The original `binary_search` over ascending upper bounds: index of the range containing `x`.
fn binary_search(arr: &[f64], x: f64) -> Option<usize> {
    let (mut low, mut high) = (0i64, arr.len() as i64 - 1);
    while low <= high {
        let mid = ((high + low) / 2) as usize;
        if mid + 1 == arr.len() {
            return if arr[mid] >= x { Some(mid) } else { None };
        }
        if arr[mid] <= x && arr[mid + 1] > x {
            return Some(mid);
        } else if arr[mid] < x {
            low = mid as i64 + 1;
        } else if arr[mid] > x {
            high = mid as i64 - 1;
        } else {
            // arr[mid] == x and arr[mid+1] <= x: the original loops forever here; stop instead.
            return Some(mid);
        }
    }
    None
}

/// `__heatmap_make` + `__annotate_heatmap`: imshow with vmin 0.5, plasma colormap, colorbar,
/// category ticks, and optionally the count in every cell.
fn heatmap_figure(
    grid: Vec<Vec<f64>>,
    row_labels: &[String],
    col_labels: &[String],
    annotate: bool,
    title: &str,
    out: &mut Out,
) -> Figure {
    let mut ax = Axes::new();
    ax.title = title.to_string();
    let rows = grid.len();
    let cols = grid.first().map_or(0, Vec::len);
    ax.xticks = Some((0..cols).map(|i| i as f64).collect());
    ax.yticks = Some((0..rows).map(|i| i as f64).collect());
    ax.xticklabels = Some(col_labels.to_vec());
    ax.yticklabels = Some(row_labels.to_vec());
    ax.xtick_rotation = Some(-30.0);
    ax.colorbar = Some("Frequency".into());
    let max = grid.iter().flatten().copied().fold(f64::NAN, f64::max);
    let mut texts = Vec::new();
    if annotate {
        if max.is_nan() || max < 0.5 {
            out.diag("heatmap has no values >= 0.5; skipped annotations (matplotlib raises here)".into());
        } else {
            let norm = |v: f64| (v - 0.5) / (max - 0.5);
            let threshold = norm(max) / 2.0;
            for (i, row) in grid.iter().enumerate() {
                for (j, v) in row.iter().enumerate() {
                    let color = if norm(*v) < threshold { "white" } else { "black" };
                    texts.push(Call::Text {
                        x: j as f64,
                        y: i as f64,
                        text: format!("{}", *v as i64),
                        color: color.into(),
                    });
                }
            }
        }
    }
    ax.push(Call::Imshow { data: grid, vmin: 0.5, cmap: "plasma".into() });
    ax.calls.extend(texts);
    Figure::new(ax)
}

// ---------------------------------------------------------------------------------------------
// ASCII tables (summary.py, percentiles.py)
// ---------------------------------------------------------------------------------------------

/// summary.py `__string_const_chars`: first `n` chars, right-padded with spaces.
fn pad_right(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() >= n {
        chars[..n].iter().collect()
    } else {
        format!("{}{}", s, " ".repeat(n - chars.len()))
    }
}

/// percentiles.py `__string_const_chars`: first `n` chars, left-padded with spaces.
fn pad_left(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() >= n {
        chars[..n].iter().collect()
    } else {
        format!("{}{}", " ".repeat(n - chars.len()), s)
    }
}

/// `float_constant_chars`: `"%9.4f"`, left-padded to `length`.
fn float_constant_chars(v: f64, length: usize) -> String {
    let s = pyfmt::percent_f(v, 9, 4);
    let n = s.chars().count();
    if n >= length {
        s
    } else {
        format!("{}{}", " ".repeat(length - n), s)
    }
}

/// `print_summary` for one series. `timestamps` enables the throughput column.
fn print_summary(
    pauses: &[Option<f64>],
    label: &str,
    title: bool,
    timestamps: Option<&[Option<f64>]>,
    numpy: bool,
    out: &mut Out,
) -> R<()> {
    let p = values(pauses);
    let (max_pause, sum_pauses, average, std) = if p.is_empty() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        let max = p.iter().copied().fold(f64::NEG_INFINITY, |a, b| if b > a { b } else { a });
        let sum = pyfmt::round(p.iter().fold(0.0, |a, b| a + b), 4);
        let avg = pyfmt::round(sum / p.len() as f64, 4);
        (round_like(max, 4, numpy), sum, avg, stats::np_round(stats::std(&p), 4))
    };
    let mut throughput = None;
    if let Some(ts) = timestamps.filter(|t| !t.is_empty()) {
        let last = ts[ts.len() - 1].ok_or("TypeError: unsupported operand (None)")?;
        if last == 0.0 && !numpy {
            return Err("ZeroDivisionError: float division by zero".into());
        }
        let raw = ((last * 1000.0) - sum_pauses) / (last * 1000.0);
        throughput = Some(round_like(raw, 4, numpy) * 100.0);
    }
    let throughput = throughput.filter(|t| *t != 0.0);
    if title {
        let mut t = String::from("  summary (ms)   | ");
        t += "Event Count   | ";
        t += "Max Duration  | ";
        t += "Sum Duration  | ";
        t += "Mean Duration | ";
        t += "Std Dev.      |";
        if throughput.is_some() {
            t += " Throughput    |";
        }
        out.println(&t);
        out.println(&"-".repeat(t.chars().count()));
    }
    let label = if label.is_empty() { "Run:" } else { label };
    let chars: Vec<char> = label.chars().collect();
    let label: String = chars[chars.len().saturating_sub(15)..].iter().collect();
    let mut line = pad_right(&label, 15) + " | ";
    for v in [p.len() as f64, max_pause, sum_pauses, average, std] {
        line += &float_constant_chars(v, 13);
        line += " | ";
    }
    if let Some(t) = throughput {
        line += &float_constant_chars(round_like(t, 7, numpy), 13);
        line += " % ";
    }
    out.println(&line);
    Ok(())
}

fn compare_summary(
    lists: &[Vec<Option<f64>>],
    labels: &[String],
    timestamps: Option<&[Vec<Option<f64>>]>,
    numpy: bool,
    out: &mut Out,
) -> R<()> {
    if lists.is_empty() {
        out.println("No pauses_ms_lists in compare_summary.");
        return Ok(());
    }
    let labels: Vec<String> =
        if labels.is_empty() { (0..lists.len()).map(|i| i.to_string()).collect() } else { labels.to_vec() };
    let ts = timestamps.filter(|t| !t.is_empty());
    for (i, l) in lists.iter().enumerate() {
        let label = labels.get(i).ok_or("IndexError: list index out of range")?;
        let stamps = match ts {
            Some(ts) => Some(ts.get(i).ok_or("IndexError: list index out of range")?.as_slice()),
            None => None,
        };
        print_summary(l, label, i == 0, stamps, numpy, out)?;
    }
    Ok(())
}

/// `plot_summary`: ASCII table of count / max / sum / mean / std (and throughput).
pub fn plot_summary(tables: &[&Table], sel: &Select, throughput: bool, out: &mut Out) -> R<()> {
    let g = filter_and_group(tables, &Select { colors: None, column_timing: None, ..sel.clone() }, out);
    if g.labels.is_empty() {
        return Ok(());
    }
    let ts = if throughput { Some(g.x.as_slice()) } else { None };
    if g.labels.iter().map(|l| l.chars().count()).max().unwrap_or(0) <= 15 {
        return compare_summary(&g.y, &g.labels, ts, g.numpy, out);
    }
    let mut series = g.y.clone();
    let mut short = Vec::new();
    let mut found = false;
    let mut index = 0;
    while index < series.len() {
        if !series[index].is_empty() {
            if !found {
                out.println("Legend: ");
            }
            found = true;
            let letter = char::from_u32('A' as u32 + index as u32).unwrap_or('?');
            out.println(&format!("{letter} | {}", g.labels[index]));
            short.push(letter.to_string());
            index += 1;
        } else {
            series.remove(index);
        }
    }
    if found {
        out.println(&"-".repeat(97));
    }
    compare_summary(&series, &short, ts, g.numpy, out)
}

/// `print_percentiles` for one series.
fn print_percentiles(pauses: &[Option<f64>], title: bool, percentiles: &[f64], label: &str, out: &mut Out) {
    let p = values(pauses);
    if p.is_empty() {
        return;
    }
    let label = if label.is_empty() { "label" } else { label };
    if title {
        let mut t = String::new();
        for q in percentiles {
            t += &pad_left(&format!("{}%", repr_num_intlike(*q)), 18);
            t += " | ";
        }
        out.println(&format!("    | {t}\n{}", "-".repeat(t.chars().count() + 12)));
    }
    out.print(&format!("{} | ", pad_left(label, 3)));
    for q in percentiles {
        let v = stats::np_round(stats::percentile(&p, *q), 4);
        out.print(&format!("{} | ", float_constant_chars(v, 18)));
    }
    out.println("");
}

pub const DEFAULT_PERCENTILES: [f64; 4] = [50.0, 95.0, 99.0, 99.99];

fn compare_pauses_percentiles(lists: &[Vec<Option<f64>>], percentiles: &[f64], labels: &[String], out: &mut Out) {
    if lists.is_empty() {
        out.println("No list_of_list_pauses_ms provided to plot_compare_percentiles");
        return;
    }
    let labels: Vec<String> =
        if labels.is_empty() { (0..lists.len()).map(|i| i.to_string()).collect() } else { labels.to_vec() };
    for (i, l) in lists.iter().enumerate() {
        print_percentiles(l, i == 0, percentiles, labels.get(i).map_or("", String::as_str), out);
    }
}

/// `plot_percentiles`: ASCII table of latency percentiles.
pub fn plot_percentiles(tables: &[&Table], sel: &Select, percentiles: Option<&[f64]>, out: &mut Out) {
    let percentiles = match percentiles {
        Some(p) if !p.is_empty() => p,
        _ => &DEFAULT_PERCENTILES[..],
    };
    let g = filter_and_group(tables, &Select { colors: None, column_timing: None, ..sel.clone() }, out);
    if g.labels.is_empty() {
        return;
    }
    if g.labels.iter().map(|l| l.chars().count()).max().unwrap_or(0) <= 3 {
        compare_pauses_percentiles(&g.y, percentiles, &g.labels, out);
        return;
    }
    if g.x.len() > g.labels.len() {
        out.println("Not enough labels to plot");
    }
    out.println("Legend (All timing in milliseconds) : ");
    let mut short = Vec::new();
    for (index, label) in g.labels.iter().enumerate() {
        let letter = char::from_u32('A' as u32 + index as u32).unwrap_or('?');
        out.println(&format!("{letter} | {label}"));
        short.push(letter.to_string());
    }
    out.println(&"-".repeat(97));
    compare_pauses_percentiles(&g.y, percentiles, &short, out);
}

/// Convenience used by analyses: a filter that keeps `EventType == "Pause"`.
pub fn pauses_only() -> Filter {
    Filter::eq("EventType", "Pause")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_mod() {
        assert_eq!(py_mod(7.5, 10.0), 7.5);
        assert_eq!(py_mod(-3.0, 10.0), 7.0);
        assert_eq!(py_mod(20.0, 10.0), 0.0);
    }

    #[test]
    fn original_binary_search() {
        let r = [1.0, 4.0, 16.0, 64.0];
        assert_eq!(binary_search(&r, 0.5), None);
        assert_eq!(binary_search(&r, 1.0), Some(0));
        assert_eq!(binary_search(&r, 5.0), Some(1));
        assert_eq!(binary_search(&r, 64.0), Some(3));
        assert_eq!(binary_search(&r, 65.0), None);
    }
}
