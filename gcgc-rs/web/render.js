// Turns the engine's chart description (recorded matplotlib-style calls) into Plotly traces.

const MARKER_PLOT_SIZE = 5; // matplotlib plot(marker="o", markersize=3) in points ≈ 5 px
const SCATTER_SIZE = 7; // matplotlib scatter default s=36 pt² ≈ 6 pt diameter
// matplotlib's "plasma" colormap (Plotly.js has no built-in equivalent).
const PLASMA = [
  [0.0, "#0d0887"], [0.111, "#46039f"], [0.222, "#7201a8"], [0.333, "#9c179e"], [0.444, "#bd3786"],
  [0.556, "#d8576b"], [0.667, "#ed7953"], [0.778, "#fb9f3a"], [0.889, "#fdca26"], [1.0, "#f0f921"],
];

export function cssColor(c, fallback = "#888") {
  if (c == null) return fallback;
  if (typeof c === "string") return c;
  const [r, g, b] = c.map((v) => Math.round(v * 255));
  return `rgb(${r},${g},${b})`;
}

function xyTrace(call, mode, size) {
  const trace = {
    type: "scatter",
    mode,
    x: call.x,
    y: call.y,
    name: call.label ?? "",
    showlegend: call.label != null && !call.label.startsWith("_"),
    legendgroup: call.label ?? undefined,
    opacity: call.alpha ?? 1,
    connectgaps: false,
    hovertemplate: `${call.label ? escapeHtml(call.label) + "<br>" : ""}x=%{x}<br>y=%{y}<extra></extra>`,
  };
  const color = cssColor(call.color);
  if (mode === "markers") trace.marker = { color, size };
  else trace.line = { color, width: 1.6 };
  return trace;
}

function escapeHtml(s) {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}

/** Group consecutive matplotlib bar() calls into one Plotly bar trace per (color, width). */
function barTraces(bars) {
  const groups = new Map();
  for (const b of bars) {
    const key = `${JSON.stringify(b.color)}|${b.width ?? ""}|${b.alpha ?? ""}`;
    let g = groups.get(key);
    if (!g) {
      g = { x: [], y: [], label: null, color: b.color, width: b.width, alpha: b.alpha, widths: [] };
      groups.set(key, g);
    }
    g.x.push(b.x);
    g.y.push(b.height);
    g.widths.push(b.width ?? 0.8);
    if (g.label == null && b.label != null) g.label = b.label;
  }
  return [...groups.values()].map((g) => ({
    type: "bar",
    x: g.x,
    y: g.y,
    width: g.widths,
    name: g.label ?? "",
    showlegend: g.label != null,
    marker: { color: cssColor(g.color) },
    opacity: g.alpha ?? 1,
    // matplotlib bar(x) centres the bar on x (align="center"), as Plotly does.
    hovertemplate: `${g.label ? escapeHtml(g.label) + "<br>" : ""}%{y}<extra></extra>`,
  }));
}

function heatmapTrace(call, theme, blankEmpty = false) {
  // Like the original's matplotlib output: values below vmin (empty cells) take the colormap's lowest
  // colour. (The original calls cmap.set_under("white"), but on a different colormap object after
  // drawing, so it has no effect.) Plotly clamps values below zmin to the lowest colour.
  const z = blankEmpty ? call.data.map((row) => row.map((v) => (v < call.vmin ? null : v))) : call.data;
  const max = Math.max(call.vmin, ...call.data.flat());
  return {
    type: "heatmap",
    z,
    zmin: call.vmin,
    zmax: max,
    colorscale: call.cmap === "plasma" ? PLASMA : call.cmap,
    xgap: 2,
    ygap: 2,
    colorbar: { title: { text: "Frequency", side: "right" }, outlinewidth: 0 },
    hovertemplate: "count %{z}<extra></extra>",
    hoverongaps: false,
    showscale: true,
  };
}

/**
 * Build Plotly {data, layout} for one figure.
 * theme: {fg, bg, grid, muted, font, narrow}
 */
/**
 * opts.sparseHeatmap: draw heatmaps with empty cells left blank and stretched to the full width
 * (used for the linear latency heatmaps). Otherwise empty cells take plasma's lowest colour and
 * cells are square, like matplotlib.
 */
export function toPlotly(fig, theme, opts = {}) {
  const ax = fig.axes;
  const data = [];
  const annotations = [];
  const bars = [];
  let heatmap = false;
  for (const call of ax.calls) {
    switch (call.method) {
      case "plot": {
        const markers = call.marker != null && call.linestyle === "";
        data.push(xyTrace(call, markers ? "markers" : "lines", MARKER_PLOT_SIZE));
        break;
      }
      case "scatter":
        data.push(xyTrace(call, "markers", SCATTER_SIZE));
        break;
      case "bar":
        bars.push(call);
        break;
      case "imshow":
        heatmap = true;
        data.push(heatmapTrace(call, theme, !!opts.sparseHeatmap));
        break;
      case "text":
        annotations.push({
          x: call.x,
          y: call.y,
          text: call.text,
          showarrow: false,
          font: { color: call.color, size: 10 },
        });
        break;
    }
  }
  if (bars.length) data.unshift(...barTraces(bars));

  const axisBase = {
    gridcolor: theme.grid,
    zerolinecolor: theme.grid,
    linecolor: theme.muted,
    tickcolor: theme.muted,
    automargin: true,
    title: { font: { size: 12 } },
  };
  const xaxis = { ...axisBase, title: { text: ax.xlabel || "", font: { size: 12 } } };
  const yaxis = { ...axisBase, title: { text: ax.ylabel || "", font: { size: 12 } } };
  if (ax.yscale === "log") yaxis.type = "log";
  if (ax.xticks && ax.xticklabels) {
    xaxis.tickmode = "array";
    xaxis.tickvals = ax.xticks;
    xaxis.ticktext = ax.xticklabels;
  }
  if (ax.yticks && ax.yticklabels) {
    yaxis.tickmode = "array";
    yaxis.tickvals = ax.yticks;
    yaxis.ticktext = ax.yticklabels;
  }
  // matplotlib rotation is counter-clockwise; Plotly's tickangle is clockwise.
  if (ax.xtick_rotation != null) xaxis.tickangle = -ax.xtick_rotation;
  if (heatmap) {
    // matplotlib imshow keeps square cells (aspect="equal").
    if (!opts.sparseHeatmap) {
      yaxis.scaleanchor = "x";
      yaxis.scaleratio = 1;
      yaxis.constrain = "domain";
      xaxis.constrain = "domain";
    }
    yaxis.autorange = "reversed";
    xaxis.showgrid = false;
    yaxis.showgrid = false;
    xaxis.zeroline = false;
    yaxis.zeroline = false;
  }
  if (bars.length) xaxis.zeroline = false;

  const showLegend = ax.legend && fig.legend_labels.length > 0;
  const layout = {
    title: { text: escapeHtml(ax.title || ""), font: { size: 14 }, x: 0, xanchor: "left", pad: { l: 4 } },
    xaxis,
    yaxis,
    annotations,
    showlegend: showLegend,
    // Wide screens: legend outside on the right (like the notebook); narrow screens: below the plot.
    legend: theme.narrow
      ? { orientation: "h", x: 0, y: -0.25, xanchor: "left", yanchor: "top", font: { size: 11 } }
      : { orientation: "v", x: 1.02, y: 1, xanchor: "left", yanchor: "top", font: { size: 11 } },
    barmode: "overlay",
    bargap: 0,
    margin: { l: 64, r: 16, t: 40, b: 56 },
    paper_bgcolor: theme.bg,
    plot_bgcolor: theme.bg,
    font: { color: theme.fg, family: theme.font, size: 12 },
    hoverlabel: { font: { family: theme.font } },
    height: heatmap ? (opts.sparseHeatmap ? 460 : 600) : 420,
    autosize: true,
  };
  return { data, layout };
}
