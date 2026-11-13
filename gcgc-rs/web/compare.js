// Side-by-side: golden matplotlib figures from the original Python notebook vs the wasm engine's charts.
// Must be served from the repository root (./serve.sh) so ../../port-notes/golden and ../../datasets resolve.
import init, { Session } from "./pkg/gcgc_wasm.js";
import { toPlotly } from "./render.js";

const GOLDEN = "../../port-notes/golden/notebooks/";
const DATASETS = "../../datasets/";

const NOTEBOOKS = {
  GCGC: [
    ["Serial", ["SerialGC.log"]], ["Parallel", ["ParallelGC.log"]], ["G1", ["G1GC.log"]],
    ["Shenandoah", ["ShenandoahGC.log"]], ["ZGC", ["ZGC.log.0", "ZGC.log.1"]],
    ["GenZGC", ["GenZGC.log"]], ["GraalVM", ["GraalVM.log"]],
  ],
  FeatureExamples: [
    ["Serial", ["SerialGCMetaspace.log"]], ["Parallel", ["ParallelGCMetaspace.log"]], ["G1", ["G1GCMetaspace.log"]],
    ["Shenandoah", ["ShenandoahGCMetaspace.log"]], ["ZGC", ["ZGCMetaspace.log"]], ["Code Cache", ["SegmentedCodeCache.log"]],
  ],
};

// notebook cell -> analysis id (same mapping as the parity tests)
const CELLS = [
  [7, "stw_pauses"], [9, "stw_pauses_by_name"], [11, "latency_heatmap"], [13, "latency_heatmap_log"],
  [15, "latency_percentiles"], [17, "pause_sum_intervals"], [20, "heap_percent_full"], [22, "heap_before_after"],
  [24, "reclaimed"], [26, "allocation_rate"], [30, "code_heap_non_profiled"], [32, "code_heap_profiled"],
  [34, "code_heap_non_nmethods"], [36, "code_cache_flushing"], [39, "metaspace"], [42, "concurrent_durations"],
  [44, "event_duration_sums"], [46, "pause_summary"], [48, "pause_percentiles"], [50, "event_mean_sum"],
  [52, "pause_histogram"], [54, "gc_frequency"], [56, "external_overlay"],
];

const status = (t) => (document.getElementById("status").textContent = t);

async function text(url) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status} (serve the repo root with ./serve.sh and run make_golden.sh)`);
  return r.text();
}

function theme() {
  const cs = getComputedStyle(document.documentElement);
  const v = (n) => cs.getPropertyValue(n).trim();
  return { fg: v("--fg"), bg: v("--surface"), grid: v("--grid"), muted: v("--muted"), font: v("--font"), narrow: false };
}

async function show(nb) {
  const rows = document.getElementById("rows");
  rows.replaceChildren();
  status("Parsing logs…");
  const session = new Session();
  for (const [label, files] of NOTEBOOKS[nb]) {
    const texts = [];
    for (const f of files) texts.push(await text(DATASETS + f));
    session.addRun(label, files, texts, true, false);
  }
  session.setExternal("Reads completed by application", await text(DATASETS + "example_external_data.csv"), "Time Seconds", "Reads Completed");
  const catalogue = Object.fromEntries(JSON.parse(Session.catalogue()).map((a) => [a.id, a]));
  status("Rendering…");
  for (const [cell, id] of CELLS) {
    const golden = JSON.parse(await text(`${GOLDEN}${nb}/cell_${String(cell).padStart(2, "0")}.json`));
    const ours = JSON.parse(session.analyse(id, ""));
    const row = document.createElement("section");
    row.className = "row";
    const info = catalogue[id];
    const sameText = (ours.stdout ?? "") === golden.stdout;
    row.innerHTML = `<h3>#${info.number} ${info.title}<small>notebook cell ${cell} · ${golden.figures.length} figure(s) both sides · printed output
      <span class="${sameText ? "same" : "diff"}">${sameText ? "identical" : "differs"}</span></small></h3>
      <div class="pair"><div class="side py"><h4>Python (matplotlib)</h4></div><div class="side rs"><h4>Rust / WebAssembly (Plotly)</h4></div></div>`;
    rows.append(row);
    const py = row.querySelector(".py");
    const rs = row.querySelector(".rs");
    golden.figures.forEach((_, k) => {
      const img = new Image();
      img.loading = "lazy";
      img.alt = `Python figure ${k + 1} for cell ${cell}`;
      img.src = `${GOLDEN}${nb}/cell_${String(cell).padStart(2, "0")}_fig${k}.png`;
      py.append(img);
    });
    if (golden.stdout) py.append(Object.assign(document.createElement("pre"), { textContent: golden.stdout }));
    if (ours.error) rs.append(Object.assign(document.createElement("p"), { className: "error", textContent: ours.error }));
    for (const fig of ours.figures ?? []) {
      const el = document.createElement("div");
      rs.append(el);
      const { data, layout } = toPlotly(fig, theme(), { sparseHeatmap: id === "latency_heatmap" });
      if (!fig.axes.calls.some((c) => c.method === "imshow")) layout.height = 340;
      await Plotly.newPlot(el, data, layout, { displaylogo: false, responsive: true });
    }
    if (ours.stdout) rs.append(Object.assign(document.createElement("pre"), { textContent: ours.stdout }));
  }
  status("Done.");
}

await init();
const sel = document.getElementById("nb");
sel.addEventListener("change", () => show(sel.value));
const wait = () => new Promise((r) => (window.Plotly ? r() : setTimeout(() => wait().then(r), 20)));
await wait();
show(sel.value).catch((e) => status(String(e)));
