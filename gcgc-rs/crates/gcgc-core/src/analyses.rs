//! The analyses of the GCGC notebook (`src/notebooks/GCGC.ipynb`), one function per numbered section.
//!
//! Each analysis uses the notebook's parameters by default; [`Params`] lets a UI override them.
//! Like the notebook, analyses from #8 on see the runs sorted by time (cell 22 sorts the shared
//! dataframes in place, and every later cell plots from the sorted data).

use crate::chart::{Axes, Color, Figure, Scale};
use crate::out::Out;
use crate::plotting::{self as p, GroupingFn, HeatmapDims, LogHeatmapDims, R};
use crate::pyfmt::repr_num_intlike;
use crate::query::{Filter, Select};
use crate::run::CODE_HEAP_COLUMNS;
use crate::table::Table;
use serde::{Deserialize, Serialize};

/// Optional overrides for an analysis. Unset fields use the notebook's values.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Params {
    /// Row filter (replaces the analysis' default filter).
    pub filter: Option<Filter>,
    /// Column to group by; `""` means no grouping.
    pub group_by: Option<String>,
    /// Column plotted on the y axis (for analyses that take one).
    pub column: Option<String>,
    /// Interval length in seconds (or milliseconds for the pause histogram).
    pub interval: Option<f64>,
    pub percentiles: Option<Vec<f64>>,
    /// Percentile for the allocation-rate second line.
    pub percentile: Option<f64>,
    pub line_graph: Option<bool>,
    pub different_colors: Option<bool>,
    pub log_y: Option<bool>,
    pub annotate: Option<bool>,
    pub heatmap: Option<HeatmapDims>,
    pub log_heatmap: Option<LogHeatmapDims>,
    pub throughput: Option<bool>,
    pub remove_empty_intervals: Option<bool>,
    pub grouping: Option<GroupingFn>,
    /// Custom chart: chart type (one of [`CUSTOM_KINDS`]).
    pub kind: Option<String>,
    /// Custom chart: x column (seconds).
    pub column_timing: Option<String>,
}

/// External tabular data to overlay (analysis #21).
#[derive(Debug, Clone)]
pub struct External {
    pub label: String,
    pub table: Table,
    pub x_column: String,
    pub y_column: String,
}

/// Inputs shared by all analyses.
pub struct Ctx<'a> {
    pub labels: Vec<String>,
    /// Run tables in load order.
    pub loaded: Vec<&'a Table>,
    /// Run tables sorted by time (as after notebook cell 22).
    pub sorted: Vec<&'a Table>,
    pub external: Option<&'a External>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Output {
    pub figures: Vec<Figure>,
    /// Text the notebook cell prints (ASCII tables, warnings).
    pub stdout: String,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParamSpec {
    pub key: &'static str,
    pub label: &'static str,
    /// "number", "numbers", "bool", "column", "filter", "heatmap", "log_heatmap", "grouping"
    pub kind: &'static str,
    pub default: serde_json::Value,
    /// Choices for kind "select".
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisInfo {
    pub id: &'static str,
    pub number: &'static str,
    pub section: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub params: Vec<ParamSpec>,
}

fn spec(key: &'static str, label: &'static str, kind: &'static str, default: serde_json::Value) -> ParamSpec {
    ParamSpec { key, label, kind, default, options: Vec::new() }
}

fn select(key: &'static str, label: &'static str, options: &[&'static str]) -> ParamSpec {
    ParamSpec { key, label, kind: "select", default: serde_json::json!(options[0]), options: options.to_vec() }
}

/// Chart types offered by the custom chart.
pub const CUSTOM_KINDS: [&str; 8] =
    ["scatter", "line", "bar_sum", "bar_mean", "interval_sum", "interval_percentiles", "summary", "percentiles"];

fn pauses() -> Filter {
    p::pauses_only()
}

/// The catalogue of analyses, in notebook order.
pub fn catalogue() -> Vec<AnalysisInfo> {
    use serde_json::json;
    let filter = |f: Filter| spec("filter", "Filter", "filter", serde_json::to_value(f).unwrap());
    let group = |d: &str| spec("group_by", "Group by", "column", json!(d));
    let interval = |d: f64, label: &'static str| spec("interval", label, "number", json!(d));
    let line = |d: bool| spec("line_graph", "Line graph", "bool", json!(d));
    let logy = |d: bool| spec("log_y", "Log y axis", "bool", json!(d));
    vec![
        AnalysisInfo { id: "stw_pauses", number: "1", section: "A. Latency", title: "STW pauses during program runtime",
            description: "Every stop-the-world pause in milliseconds against application runtime.",
            params: vec![filter(pauses()), group(""), logy(false)] },
        AnalysisInfo { id: "stw_pauses_by_name", number: "2", section: "A. Latency", title: "STW pauses, grouped by EventName",
            description: "The same pauses, one series per event name (Young, Remark, Mark Start, …).",
            params: vec![filter(pauses()), group("EventName"), logy(false)] },
        AnalysisInfo { id: "latency_heatmap", number: "3", section: "A. Latency", title: "Latency heatmaps (linear)",
            description: "Frequency of pauses per time interval (x) and latency bucket (y). Set the ranges so heatmaps are comparable.",
            params: vec![filter(pauses()),
                spec("heatmap", "Buckets [x count, y count, x seconds, y ms]", "heatmap", json!({"x_buckets": 40, "y_buckets": 20, "x_seconds": 10.0, "y_size": 1.0})),
                spec("annotate", "Show counts", "bool", json!(false))] },
        AnalysisInfo { id: "latency_heatmap_log", number: "4", section: "A. Latency", title: "Latency heatmaps (logarithmic)",
            description: "Like #3 with logarithmic latency buckets; good for comparing concurrent and STW collectors.",
            params: vec![filter(pauses()),
                spec("log_heatmap", "Buckets [x count, y count, x seconds, base]", "log_heatmap", json!({"x_buckets": 10, "y_buckets": 10, "x_seconds": 50.0, "base": 4.0})),
                spec("annotate", "Show counts", "bool", json!(true))] },
        AnalysisInfo { id: "latency_percentiles", number: "5", section: "A. Latency", title: "Latency percentiles over time intervals",
            description: "Pause latency percentiles within each time interval; one line per percentile.",
            params: vec![filter(pauses()), interval(15.0, "Interval (s)"), spec("percentiles", "Percentiles", "numbers", json!([99.9, 90, 50])),
                spec("different_colors", "Unique colour per line", "bool", json!(false)), line(true)] },
        AnalysisInfo { id: "pause_sum_intervals", number: "6", section: "A. Latency", title: "Sum of pause durations over intervals",
            description: "Total time paused within each interval (similar to Java Flight Recorder's GC chart).",
            params: vec![filter(pauses()), interval(15.0, "Interval (s)"), line(true), spec("remove_empty_intervals", "Hide empty intervals", "bool", json!(false))] },
        AnalysisInfo { id: "heap_percent_full", number: "7", section: "B. Heap", title: "Percentage of heap full after global GC",
            description: "Heap occupancy after major collections; helps size the heap.",
            params: vec![] },
        AnalysisInfo { id: "heap_before_after", number: "8", section: "B. Heap", title: "Heap before and after GC",
            description: "Heap used in MB before and after each collection.",
            params: vec![line(true)] },
        AnalysisInfo { id: "reclaimed", number: "9", section: "B. Heap", title: "MB reclaimed during program runtime",
            description: "HeapBeforeGC − HeapAfterGC per collection. Can be negative when allocation outpaces collection.",
            params: vec![] },
        AnalysisInfo { id: "allocation_rate", number: "10", section: "B. Heap", title: "Heap allocation rate",
            description: "MB/s allocated between collections: mean and a percentile per interval.",
            params: vec![interval(10.0, "Interval (s)"), spec("percentile", "Percentile line", "number", json!(100)), line(true)] },
        AnalysisInfo { id: "code_heap_non_profiled", number: "11a", section: "C. Code cache", title: "Percentage used: non-profiled nmethods code heap",
            description: "used (line) and max_used (markers) as a percentage of the code heap size.", params: vec![] },
        AnalysisInfo { id: "code_heap_profiled", number: "11b", section: "C. Code cache", title: "Percentage used: profiled nmethods code heap",
            description: "used (line) and max_used (markers) as a percentage of the code heap size.", params: vec![] },
        AnalysisInfo { id: "code_heap_non_nmethods", number: "11c", section: "C. Code cache", title: "Percentage used: non-nmethods code heap",
            description: "used (line) and max_used (markers) as a percentage of the code heap size.", params: vec![] },
        AnalysisInfo { id: "code_cache_flushing", number: "12", section: "C. Code cache", title: "Code cache flushing rate",
            description: "Code cache flushes per second between flushing events.", params: vec![line(true)] },
        AnalysisInfo { id: "metaspace", number: "13", section: "D. Metaspace", title: "Used metaspace after GC",
            description: "Metaspace in use (MB) after each collection.", params: vec![] },
        AnalysisInfo { id: "concurrent_durations", number: "14", section: "E. Summaries", title: "Concurrent durations during runtime",
            description: "Duration of every concurrent phase. Empty for fully STW collectors.",
            params: vec![filter(Filter::eq("EventType", "Concurrent")), group(""), logy(false)] },
        AnalysisInfo { id: "event_duration_sums", number: "15", section: "E. Summaries", title: "Sum of event durations, grouped by EventType",
            description: "Where GC time goes: total duration per event type. Try grouping by EventName.",
            params: vec![filter(Filter::truthy("Duration_milliseconds")), group("EventType")] },
        AnalysisInfo { id: "pause_summary", number: "16", section: "E. Summaries", title: "Pauses summary",
            description: "Count, max, sum, mean, standard deviation and throughput of pauses.",
            params: vec![filter(pauses()), group(""), spec("throughput", "Throughput", "bool", json!(true))] },
        AnalysisInfo { id: "pause_percentiles", number: "17", section: "E. Summaries", title: "Pause percentiles",
            description: "Pause latency percentiles per run.",
            params: vec![filter(pauses()), group(""), spec("percentiles", "Percentiles", "numbers", json!([50, 95, 99, 99.99]))] },
        AnalysisInfo { id: "event_mean_sum", number: "18", section: "E. Summaries", title: "Mean and sum of event durations",
            description: "Sum and mean duration of pause and concurrent events.",
            params: vec![filter(Filter::one_of("EventType", &["Concurrent", "Pause"])), group("EventType")] },
        AnalysisInfo { id: "pause_histogram", number: "19", section: "E. Summaries", title: "Pause frequencies histogram",
            description: "How many pauses fall into each duration bucket (log scale).",
            params: vec![filter(pauses()), interval(10.0, "Bucket width (ms)"), logy(true)] },
        AnalysisInfo { id: "gc_frequency", number: "20", section: "E. Summaries", title: "Number of times GC invoked over time intervals",
            description: "Distinct GC cycles per interval. (An interval without GCs counts as 1, as in the original.)",
            params: vec![filter(pauses()), interval(15.0, "Interval (s)")] },
        AnalysisInfo { id: "external_overlay", number: "21", section: "Extras", title: "Overlay external data",
            description: "STW pauses with an external CSV series (e.g. application metrics) on the same axes.",
            params: vec![filter(pauses())] },
        AnalysisInfo { id: "heap_after_gc_percent", number: "T", section: "Extras", title: "Heap after GC as % of a fixed size",
            description: "Universal plotting tutorial: HeapAfterGC scaled by a grouping function (default: percent of 8192 MB).",
            params: vec![spec("grouping", "Function", "grouping", json!({"fn": "percent_of", "total": 8192.0})),
                spec("interval", "Interval (s, 0 = per event)", "number", json!(0)), line(false)] },
        AnalysisInfo { id: "custom", number: "+", section: "Extras", title: "Custom chart",
            description: "Build your own view, like writing a notebook cell: any column, filter, grouping and chart type.",
            params: vec![select("kind", "Chart type", &CUSTOM_KINDS), spec("column", "Y column", "column", json!("Duration_milliseconds")),
                spec("column_timing", "X column (time, s)", "column", json!("TimeFromStart_seconds")),
                filter(pauses()), group(""), interval(15.0, "Interval (s)"),
                spec("percentiles", "Percentiles", "numbers", json!([99, 90, 50])), logy(false)] },
    ]
}

fn finish(ax: &mut Axes, title: &str, xlabel: &str, ylabel: &str, grid: f64) {
    ax.title = title.into();
    ax.xlabel = xlabel.into();
    ax.ylabel = ylabel.into();
    ax.grid_alpha = Some(grid);
}

fn group_by<'a>(params: &'a Params, default: Option<&'a str>) -> Option<&'a str> {
    match params.group_by.as_deref() {
        Some("") => None,
        Some(g) => Some(g),
        None => default,
    }
}

fn labels_with(ctx: &Ctx, suffix: &str) -> Vec<String> {
    ctx.labels.iter().map(|l| format!("{l}{suffix}")).collect()
}

/// Run one analysis.
pub fn run(id: &str, ctx: &Ctx, params: &Params) -> R<Output> {
    let mut out = Out::new();
    let figures = run_inner(id, ctx, params, &mut out)?;
    Ok(Output { figures, stdout: out.stdout, diagnostics: out.diagnostics })
}

fn run_inner(id: &str, ctx: &Ctx, params: &Params, out: &mut Out) -> R<Vec<Figure>> {
    let pf = pauses();
    let filter = |default: &Filter| -> Filter { params.filter.clone().unwrap_or_else(|| default.clone()) };
    let labels = Some(ctx.labels.clone());
    let one = |ax: Axes| vec![Figure::new(ax)];
    let log_y = |ax: &mut Axes, default: bool| {
        if params.log_y.unwrap_or(default) {
            ax.yscale = Scale::Log;
        }
    };
    match id {
        "stw_pauses" | "stw_pauses_by_name" | "concurrent_durations" => {
            let (default_filter, default_group, title, ylabel) = match id {
                "stw_pauses" => {
                    (pf.clone(), None, "STW pauses during program runtime", "Pause duration in milliseconds")
                }
                "stw_pauses_by_name" => (
                    pf.clone(),
                    Some("EventName"),
                    "STW pauses during program runtime, sorted by EventName",
                    "Pause duration in milliseconds",
                ),
                _ => (
                    Filter::eq("EventType", "Concurrent"),
                    None,
                    "Concurrent durations during runtime",
                    "Concurrent durations in milliseconds",
                ),
            };
            let tables = if id == "concurrent_durations" { &ctx.sorted } else { &ctx.loaded };
            let f = filter(&default_filter);
            let mut ax = Axes::new();
            let sel = Select {
                filter: Some(&f),
                labels,
                group_by: group_by(params, default_group),
                column: "Duration_milliseconds",
                ..Default::default()
            };
            p::plot_scatter(&mut ax, tables, &sel, out);
            finish(&mut ax, title, "Time passed in seconds", ylabel, 0.1);
            log_y(&mut ax, false);
            Ok(one(ax))
        }
        "latency_heatmap" => {
            let f = filter(&pf);
            let dims =
                params.heatmap.unwrap_or(HeatmapDims { x_buckets: 40, y_buckets: 20, x_seconds: 10.0, y_size: 1.0 });
            let sel = Select {
                filter: Some(&f),
                labels,
                group_by: group_by(params, None),
                column: "Duration_milliseconds",
                ..Default::default()
            };
            p::plot_heatmaps(&ctx.loaded, &sel, dims, params.annotate.unwrap_or(false), out)
        }
        "latency_heatmap_log" => {
            let f = filter(&pf);
            let dims = params.log_heatmap.unwrap_or(LogHeatmapDims {
                x_buckets: 10,
                y_buckets: 10,
                x_seconds: 50.0,
                base: 4.0,
            });
            let sel = Select {
                filter: Some(&f),
                labels,
                group_by: group_by(params, None),
                column: "Duration_milliseconds",
                ..Default::default()
            };
            p::plot_heatmaps_logarithmic(&ctx.loaded, &sel, dims, params.annotate.unwrap_or(true), out)
        }
        "latency_percentiles" => {
            let f = filter(&pf);
            let interval = params.interval.unwrap_or(15.0);
            let percentiles = params.percentiles.clone().unwrap_or_else(|| vec![99.9, 90.0, 50.0]);
            let mut ax = Axes::new();
            let sel = Select {
                filter: Some(&f),
                labels,
                group_by: group_by(params, None),
                column: "Duration_milliseconds",
                ..Default::default()
            };
            p::plot_percentile_intervals(
                &mut ax,
                &ctx.loaded,
                &sel,
                interval,
                &percentiles,
                params.line_graph.unwrap_or(true),
                Some(params.different_colors.unwrap_or(false)),
                out,
            )?;
            finish(
                &mut ax,
                &format!("Plot latency percentiles during {} second time intervals", repr_num_intlike(interval)),
                "Elapsed time in seconds",
                "Pause duration in milliseconds",
                0.1,
            );
            ax.xtick_rotation = Some(20.0);
            let list: Vec<String> = percentiles.iter().map(|x| repr_num_intlike(*x)).collect();
            out.println(&format!("Percentiles plotted:  [{}]", list.join(", ")));
            Ok(one(ax))
        }
        "pause_sum_intervals" => {
            let f = filter(&pf);
            let interval = params.interval.unwrap_or(15.0);
            let mut ax = Axes::new();
            let sel = Select {
                filter: Some(&f),
                labels,
                group_by: group_by(params, None),
                column: "Duration_milliseconds",
                ..Default::default()
            };
            p::plot_sum_intervals(
                &mut ax,
                &ctx.loaded,
                &sel,
                interval,
                params.remove_empty_intervals.unwrap_or(false),
                params.line_graph.unwrap_or(true),
                out,
            )?;
            finish(
                &mut ax,
                "Sum of pause durations in milliseconds per time interval",
                "Elapsed time in seconds",
                &format!("Sum of pause time in ms per interval of {} seconds", repr_num_intlike(interval)),
                0.1,
            );
            ax.xtick_rotation = Some(20.0);
            Ok(one(ax))
        }
        "heap_percent_full" => {
            let f = filter(&Filter::not_null("HeapPercentFull"));
            let mut ax = Axes::new();
            let sel = Select { filter: Some(&f), labels, column: "HeapPercentFull", ..Default::default() };
            p::plot_line(&mut ax, &ctx.loaded, &sel, out);
            finish(&mut ax, "% heap full after global gc", "Time passed in seconds", "Percentage", 0.2);
            Ok(one(ax))
        }
        "heap_before_after" => {
            let mut figs = Vec::new();
            for (column, suffix, title) in [
                ("HeapBeforeGC", " before gc", "Heap used in MB before gc"),
                ("HeapAfterGC", " after gc", "Heap used in MB after gc"),
            ] {
                let f = Filter::not_null(column);
                let mut ax = Axes::new();
                let sel =
                    Select { filter: Some(&f), labels: Some(labels_with(ctx, suffix)), column, ..Default::default() };
                if params.line_graph.unwrap_or(true) {
                    p::plot_line(&mut ax, &ctx.sorted, &sel, out);
                } else {
                    p::plot_scatter(&mut ax, &ctx.sorted, &sel, out);
                }
                finish(&mut ax, title, "Program runtime in seconds", "Heap memory allocated in MB", 0.1);
                figs.push(Figure::new(ax));
            }
            Ok(figs)
        }
        "reclaimed" => {
            let f = filter(&Filter::and(vec![Filter::not_null("HeapBeforeGC"), Filter::not_null("HeapAfterGC")]));
            let mut ax = Axes::new();
            let sel = Select { filter: Some(&f), labels, column: "HeapBeforeGC", ..Default::default() };
            p::plot_reclaimed_bytes(&mut ax, &ctx.sorted, &sel, out)?;
            finish(&mut ax, "MB reclaimed during program runtime", "Program runtime in seconds", "MB reclaimed", 0.1);
            Ok(one(ax))
        }
        "allocation_rate" => {
            let f = filter(&Filter::and(vec![Filter::truthy("HeapBeforeGC"), Filter::truthy("HeapAfterGC")]));
            let interval = params.interval.unwrap_or(10.0);
            let percentile = params.percentile.unwrap_or(100.0);
            let line = params.line_graph.unwrap_or(true);
            let mut ax = Axes::new();
            let sel = Select {
                filter: Some(&f),
                labels: Some(labels_with(ctx, " mean")),
                column: "HeapBeforeGC",
                ..Default::default()
            };
            p::plot_allocation_rate(&mut ax, &ctx.sorted, &sel, interval, None, line, out)?;
            let sel = Select { labels: Some(labels_with(ctx, &format!(" p{}", repr_num_intlike(percentile)))), ..sel };
            p::plot_allocation_rate(&mut ax, &ctx.sorted, &sel, interval, Some(percentile), line, out)?;
            ax.title = "Java object allocation rate in MB per second".into();
            ax.xlabel = "Time passed in seconds".into();
            ax.ylabel = "Allocation rate in MB per second".into();
            Ok(one(ax))
        }
        "code_heap_non_profiled" | "code_heap_profiled" | "code_heap_non_nmethods" => {
            let (heap, used, max_used) = match id {
                "code_heap_non_profiled" => CODE_HEAP_COLUMNS[0],
                "code_heap_profiled" => CODE_HEAP_COLUMNS[1],
                _ => CODE_HEAP_COLUMNS[2],
            };
            let mut ax = Axes::new();
            let f = Filter::not_null(used);
            p::plot_line(
                &mut ax,
                &ctx.sorted,
                &Select {
                    filter: Some(&f),
                    labels: Some(labels_with(ctx, " used")),
                    column: used,
                    ..Default::default()
                },
                out,
            );
            let f = Filter::not_null(max_used);
            p::plot_scatter(
                &mut ax,
                &ctx.sorted,
                &Select {
                    filter: Some(&f),
                    labels: Some(labels_with(ctx, " max-used")),
                    column: max_used,
                    ..Default::default()
                },
                out,
            );
            finish(
                &mut ax,
                &format!("Percentage used in {heap} code heap"),
                "Program runtime in seconds",
                "Percentage used",
                0.1,
            );
            Ok(one(ax))
        }
        "code_cache_flushing" => {
            let mut ax = Axes::new();
            p::plot_code_cache_flushing_rate(
                &mut ax,
                &ctx.sorted,
                &ctx.labels,
                None,
                params.line_graph.unwrap_or(true),
                out,
            )?;
            finish(
                &mut ax,
                "Number of code cache flushes per second between flushing events",
                "Time passed in seconds",
                "Number of code cache flushes per second",
                0.1,
            );
            Ok(one(ax))
        }
        "metaspace" => {
            let f = Filter::not_null("UsedMetaspaceAfterGC");
            let mut ax = Axes::new();
            p::plot_line(
                &mut ax,
                &ctx.sorted,
                &Select { filter: Some(&f), labels, column: "UsedMetaspaceAfterGC", ..Default::default() },
                out,
            );
            finish(
                &mut ax,
                "Used metaspace in MB after GC during program runtime",
                "Program runtime in seconds",
                "Used metaspace in MB after GC",
                0.1,
            );
            Ok(one(ax))
        }
        "event_duration_sums" => {
            let f = filter(&Filter::truthy("Duration_milliseconds"));
            let mut ax = Axes::new();
            let sel = Select {
                filter: Some(&f),
                labels,
                group_by: group_by(params, Some("EventType")),
                column: "Duration_milliseconds",
                ..Default::default()
            };
            p::plot_bar_sum(&mut ax, &ctx.sorted, &sel, out);
            ax.title = "Sum of event durations in milliseconds".into();
            ax.ylabel = "Sum of milliseconds".into();
            ax.xtick_rotation = Some(15.0);
            ax.grid_alpha = Some(0.2);
            Ok(one(ax))
        }
        "pause_summary" => {
            let f = filter(&pf);
            out.println(&format!("\t\t\tPauses during runtime\n{}", "-".repeat(114)));
            let sel = Select {
                filter: Some(&f),
                labels,
                group_by: group_by(params, None),
                column: "Duration_milliseconds",
                ..Default::default()
            };
            p::plot_summary(&ctx.sorted, &sel, params.throughput.unwrap_or(true), out)?;
            Ok(vec![])
        }
        "pause_percentiles" => {
            let f = filter(&pf);
            out.println(&format!("\t\t\tPause percentiles during runtime\n{}", "-".repeat(82)));
            let sel = Select {
                filter: Some(&f),
                labels,
                group_by: group_by(params, None),
                column: "Duration_milliseconds",
                ..Default::default()
            };
            p::plot_percentiles(&ctx.sorted, &sel, params.percentiles.as_deref(), out);
            Ok(vec![])
        }
        "event_mean_sum" => {
            let f = filter(&Filter::one_of("EventType", &["Concurrent", "Pause"]));
            let sel = Select {
                filter: Some(&f),
                labels,
                group_by: group_by(params, Some("EventType")),
                column: "Duration_milliseconds",
                ..Default::default()
            };
            let mut sum = Axes::new();
            p::plot_bar_sum(&mut sum, &ctx.sorted, &sel, out);
            sum.title = "Sum of pause duration in milliseconds of all events".into();
            sum.ylabel = "Sum of pause time in milliseconds".into();
            sum.xtick_rotation = Some(15.0);
            let mut avg = Axes::new();
            p::plot_bar_avg(&mut avg, &ctx.sorted, &sel, out)?;
            avg.title = "Mean of pause duration in milliseconds of all events".into();
            avg.ylabel = "Mean of pause time in milliseconds".into();
            avg.xtick_rotation = Some(15.0);
            Ok(vec![Figure::new(sum), Figure::new(avg)])
        }
        "pause_histogram" => {
            let f = filter(&pf);
            let mut ax = Axes::new();
            let sel = Select {
                filter: Some(&f),
                labels,
                group_by: group_by(params, None),
                column: "Duration_milliseconds",
                ..Default::default()
            };
            p::plot_frequency_intervals(&mut ax, &ctx.sorted, &sel, params.interval.unwrap_or(10.0), out)?;
            ax.xtick_rotation = Some(20.0);
            ax.xlabel = "Pauses time interval in milliseconds".into();
            ax.ylabel = "Frequency".into();
            ax.title = "Pause frequencies during full runtime".into();
            log_y(&mut ax, true);
            Ok(one(ax))
        }
        "gc_frequency" => {
            let f = filter(&pf);
            let interval = params.interval.unwrap_or(15.0);
            let mut ax = Axes::new();
            let sel = Select {
                filter: Some(&f),
                labels,
                group_by: group_by(params, None),
                column: "GCIndex",
                ..Default::default()
            };
            p::plot_frequency_of_gc_intervals(&mut ax, &ctx.sorted, &sel, interval, out)?;
            finish(
                &mut ax,
                "Number of times GC invoked during time intervals",
                "Elapsed time in seconds",
                "Frequency of GC run",
                0.1,
            );
            ax.xtick_rotation = Some(20.0);
            Ok(one(ax))
        }
        "external_overlay" => {
            let f = filter(&pf);
            let mut ax = Axes::new();
            let sel = Select { filter: Some(&f), labels, column: "Duration_milliseconds", ..Default::default() };
            p::plot_scatter(&mut ax, &ctx.sorted, &sel, out);
            finish(
                &mut ax,
                "STW pauses with external data",
                "Elapsed time in seconds",
                "Pause duration in milliseconds",
                0.1,
            );
            if let Some(ext) = ctx.external {
                let sel = Select {
                    labels: Some(vec![ext.label.clone()]),
                    column: &ext.y_column,
                    column_timing: Some(&ext.x_column),
                    colors: Some(vec![Color::Named("blue".into())]),
                    ..Default::default()
                };
                p::plot_scatter(&mut ax, &[&ext.table], &sel, out);
            } else {
                out.diag("No external CSV loaded; showing pauses only.".into());
            }
            Ok(one(ax))
        }
        "heap_after_gc_percent" => {
            let f = Filter::not_null("HeapAfterGC");
            let mut ax = Axes::new();
            let sel = Select {
                filter: Some(&f),
                labels,
                column: "HeapAfterGC",
                column_timing: Some("TimeFromStart_seconds"),
                ..Default::default()
            };
            let grouping = params.grouping.unwrap_or(GroupingFn::PercentOf { total: 8192.0 });
            let interval = params.interval.filter(|i| *i > 0.0);
            p::plot_scatter_universal(
                &mut ax,
                &ctx.loaded,
                &sel,
                interval,
                grouping,
                params.line_graph.unwrap_or(false),
                out,
            )?;
            ax.title = "HeapAfterGC (grouping function applied)".into();
            ax.xlabel = "Time passed in seconds".into();
            Ok(one(ax))
        }
        "custom" => {
            let f = filter(&pf);
            let column = params.column.as_deref().filter(|c| !c.is_empty()).unwrap_or("Duration_milliseconds");
            let timing = params.column_timing.as_deref().filter(|c| !c.is_empty()).unwrap_or("TimeFromStart_seconds");
            let kind = params.kind.as_deref().unwrap_or("scatter");
            let interval = params.interval.unwrap_or(15.0);
            let percentiles = params.percentiles.clone().unwrap_or_else(|| vec![99.0, 90.0, 50.0]);
            let sel = Select {
                filter: Some(&f),
                labels,
                group_by: group_by(params, None),
                column,
                column_timing: Some(timing),
                ..Default::default()
            };
            let mut ax = Axes::new();
            match kind {
                "scatter" => p::plot_scatter(&mut ax, &ctx.sorted, &sel, out),
                "line" => p::plot_line(&mut ax, &ctx.sorted, &sel, out),
                "bar_sum" => p::plot_bar_sum(&mut ax, &ctx.sorted, &sel, out),
                "bar_mean" => p::plot_bar_avg(&mut ax, &ctx.sorted, &sel, out)?,
                "interval_sum" => p::plot_sum_intervals(&mut ax, &ctx.sorted, &sel, interval, false, true, out)?,
                "interval_percentiles" => {
                    p::plot_percentile_intervals(&mut ax, &ctx.sorted, &sel, interval, &percentiles, true, Some(false), out)?
                }
                "summary" => {
                    p::plot_summary(&ctx.sorted, &sel, true, out)?;
                    return Ok(vec![]);
                }
                "percentiles" => {
                    p::plot_percentiles(&ctx.sorted, &sel, Some(&percentiles), out);
                    return Ok(vec![]);
                }
                other => return Err(format!("unknown chart type {other:?}")),
            }
            let bars = matches!(kind, "bar_sum" | "bar_mean");
            ax.title = match kind {
                "bar_sum" => format!("Sum of {column}"),
                "bar_mean" => format!("Mean of {column}"),
                "interval_sum" => format!("Sum of {column} per {} s interval", repr_num_intlike(interval)),
                "interval_percentiles" => format!("Percentiles of {column} per {} s interval", repr_num_intlike(interval)),
                _ => format!("{column} over {timing}"),
            };
            ax.ylabel = column.to_string();
            if bars {
                ax.xtick_rotation = Some(15.0);
            } else {
                ax.xlabel = timing.to_string();
            }
            ax.grid_alpha = Some(0.1);
            log_y(&mut ax, false);
            Ok(one(ax))
        }
        other => Err(format!("unknown analysis {other:?}")),
    }
}
