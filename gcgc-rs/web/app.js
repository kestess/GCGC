// GCGC web front end: manages runs, settings and rendering. The analysis engine runs in a worker.
import { toPlotly } from "./render.js";

// ---------------------------------------------------------------------------------------------
// Worker RPC
// ---------------------------------------------------------------------------------------------
const worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
let nextId = 1;
const pending = new Map();
worker.onmessage = (e) => {
  const { id, ok, result, error } = e.data;
  const p = pending.get(id);
  if (!p) return;
  pending.delete(id);
  ok ? p.resolve(result) : p.reject(new Error(error));
};
worker.onerror = (e) => setStatus(`Engine failed to start: ${e.message || e}`, true);
function call(cmd, args) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    worker.postMessage({ id, cmd, args });
  });
}

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------
const PRESET_COLORS = [
  [230, 25, 75], [60, 180, 75], [215, 215, 25], [0, 130, 200], [245, 130, 48], [145, 30, 180],
  [70, 240, 240], [240, 50, 230], [210, 245, 60], [250, 190, 212], [0, 128, 128], [220, 190, 255],
  [170, 110, 40], [255, 250, 200], [128, 0, 0], [170, 255, 195], [128, 128, 0], [255, 215, 180],
  [0, 0, 128], [128, 128, 128], [0, 0, 0],
];

const EXAMPLES = [
  { label: "Serial", files: ["SerialGC.log"] },
  { label: "Parallel", files: ["ParallelGC.log"] },
  { label: "G1", files: ["G1GC.log"] },
  { label: "Shenandoah", files: ["ShenandoahGC.log"] },
  { label: "ZGC", files: ["ZGC.log.0", "ZGC.log.1"] },
  { label: "GenZGC", files: ["GenZGC.log"] },
  { label: "GraalVM", files: ["GraalVM.log"] },
];
const EXAMPLE_CSV = "example_external_data.csv";
// Example logs are served from the repository's datasets/ folder (or a copied ./datasets/).
const DATASET_BASES = ["./datasets/", "../../datasets/"]; // deployed site first, then the repo layout

const state = {
  runs: [], // {label, files: [{name, file?|text?}], summary}
  catalogue: [],
  params: {}, // analysis id -> params object
  stale: new Set(),
  visible: new Set(),
  rendering: new Map(),
  external: null, // {label, file?|text?, columns, x, y}
  generation: 0,
};

const $ = (sel) => document.querySelector(sel);

function setStatus(msg, isError = false) {
  const el = $("#run-status");
  el.textContent = msg;
  el.classList.toggle("error", isError);
}

function runColor(i) {
  const [r, g, b] = PRESET_COLORS[i % PRESET_COLORS.length];
  return `rgb(${r},${g},${b})`;
}

function fmtSeconds(s) {
  if (s == null) return "–";
  if (s >= 3600) return `${(s / 3600).toFixed(1)} h`;
  if (s >= 60) return `${(s / 60).toFixed(1)} min`;
  return `${s.toFixed(1)} s`;
}

// ---------------------------------------------------------------------------------------------
// Runs
// ---------------------------------------------------------------------------------------------
function options() {
  return { zeroTimes: $("#opt-zero").checked, ignoreCrashes: $("#opt-crash").checked };
}

/** Group dropped files into runs: rotated logs (name.0, name.1, …) share a run. */
function groupFiles(files) {
  const groups = new Map();
  for (const file of files) {
    const base = file.name.replace(/\.\d+$/, "");
    if (!groups.has(base)) groups.set(base, []);
    groups.get(base).push({ name: file.name, file });
  }
  return [...groups.entries()].map(([base, fs]) => ({
    label: base.replace(/\.(log|txt|out)$/i, "") || base,
    files: fs.sort((a, b) => a.name.localeCompare(b.name)),
  }));
}

async function addRuns(groups) {
  if (!groups.length) return;
  const total = groups.reduce((n, g) => n + g.files.length, 0);
  setStatus(`Parsing ${total} file${total === 1 ? "" : "s"}…`);
  const started = performance.now();
  let rows = 0;
  for (const g of groups) {
    const run = { label: uniqueLabel(g.label), files: g.files, summary: null };
    state.runs.push(run);
    renderRuns();
    try {
      run.summary = await call("addRun", { label: run.label, files: run.files, ...options() });
      rows += run.summary.rows;
    } catch (err) {
      state.runs.splice(state.runs.indexOf(run), 1);
      setStatus(`Could not parse ${g.label}: ${err.message}`, true);
      renderRuns();
      return;
    }
    renderRuns();
  }
  const ms = performance.now() - started;
  setStatus(`Parsed ${rows.toLocaleString()} events in ${(ms / 1000).toFixed(2)} s.`);
  invalidateAll();
}

function uniqueLabel(label) {
  const used = new Set(state.runs.map((r) => r.label));
  if (!used.has(label)) return label;
  for (let i = 2; ; i++) if (!used.has(`${label} (${i})`)) return `${label} (${i})`;
}

async function rebuildAllRuns() {
  const runs = state.runs.map((r) => ({ label: r.label, files: r.files }));
  await call("clear");
  state.runs = [];
  if (state.external) await sendExternal();
  await addRuns(runs);
}

function renderRuns() {
  const list = $("#run-list");
  list.replaceChildren();
  state.runs.forEach((run, i) => {
    const li = document.createElement("li");
    li.className = "run" + (run.summary ? "" : " loading");
    const sw = document.createElement("span");
    sw.className = "swatch";
    sw.style.background = runColor(i);
    const label = document.createElement("input");
    label.className = "run-label";
    label.type = "text";
    label.value = run.label;
    label.setAttribute("aria-label", "Run label");
    label.addEventListener("change", async () => {
      const v = label.value.trim() || run.label;
      label.value = v;
      run.label = v;
      await call("renameRun", { index: i, label: v });
      invalidateAll();
    });
    const actions = document.createElement("span");
    actions.className = "actions";
    actions.append(
      iconButton("↑", "Move up", i === 0, () => moveRun(i, i - 1)),
      iconButton("↓", "Move down", i === state.runs.length - 1, () => moveRun(i, i + 1)),
      iconButton("⇩", "Download event table (CSV)", !run.summary, () => exportRun(i)),
      iconButton("✕", "Remove run", !run.summary, () => removeRun(i)),
    );
    const meta = document.createElement("div");
    meta.className = "meta";
    const s = run.summary;
    meta.textContent = s
      ? `${run.files.map((f) => f.name).join(", ")} · ${s.rows.toLocaleString()} events · ${s.pauses.toLocaleString()} pauses · ${fmtSeconds(s.last_time != null && s.first_time != null ? s.last_time - s.first_time : null)}`
      : `${run.files.map((f) => f.name).join(", ")} · parsing…`;
    if (s && s.rows === 0) meta.textContent += " · no GC events recognised";
    if (s && (s.diagnostics.length || s.stdout)) meta.title = [s.stdout, ...s.diagnostics].filter(Boolean).join("\n");
    li.append(sw, label, actions, meta);
    list.append(li);
  });
  $("#empty").hidden = state.runs.length > 0;
  $("#analyses").hidden = state.runs.length === 0;
}

function iconButton(text, title, disabled, onClick) {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "icon-btn";
  b.textContent = text;
  b.title = title;
  b.setAttribute("aria-label", title);
  b.disabled = disabled;
  b.addEventListener("click", onClick);
  return b;
}

async function moveRun(from, to) {
  const [r] = state.runs.splice(from, 1);
  state.runs.splice(to, 0, r);
  await call("moveRun", { from, to });
  renderRuns();
  invalidateAll();
}

async function removeRun(i) {
  state.runs.splice(i, 1);
  await call("removeRun", { index: i });
  renderRuns();
  invalidateAll();
}

async function exportRun(i) {
  const csv = await call("exportCsv", { index: i });
  const blob = new Blob([csv], { type: "text/csv" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `${state.runs[i].label.replace(/[^\w.-]+/g, "_")}_events.csv`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

let datasetBase = null;
async function fetchDataset(name) {
  const bases = datasetBase ? [datasetBase] : DATASET_BASES;
  for (const base of bases) {
    try {
      const res = await fetch(base + name, { cache: "force-cache" });
      if (res.ok) {
        datasetBase = base;
        return await res.text();
      }
    } catch (_) {
      /* try next base */
    }
  }
  throw new Error(`example file ${name} not found (serve the repository root, see README)`);
}

async function loadExamples() {
  const btn = $("#examples");
  btn.disabled = true;
  setStatus("Downloading example logs…");
  try {
    const groups = [];
    for (const ex of EXAMPLES) {
      const files = [];
      for (const name of ex.files) files.push({ name, text: await fetchDataset(name) });
      groups.push({ label: ex.label, files });
    }
    await call("clear");
    state.runs = [];
    await addRuns(groups);
    if (!state.external) {
      try {
        const text = await fetchDataset(EXAMPLE_CSV);
        state.external = { label: "Reads completed by application", text, columns: [], x: "Time Seconds", y: "Reads Completed" };
        await sendExternal();
      } catch (_) {
        /* optional */
      }
    }
  } catch (err) {
    setStatus(err.message, true);
  } finally {
    btn.disabled = false;
  }
}

// ---------------------------------------------------------------------------------------------
// External CSV
// ---------------------------------------------------------------------------------------------
async function sendExternal() {
  const ext = state.external;
  const res = await call("setExternal", { label: ext.label, file: ext.file, text: ext.text, x: ext.x, y: ext.y });
  if (res.error) throw new Error(res.error);
  ext.columns = res.columns;
  const fixX = !ext.columns.includes(ext.x);
  const fixY = !ext.columns.includes(ext.y);
  if (fixX) ext.x = ext.columns[0] ?? "";
  if (fixY) ext.y = ext.columns[1] ?? ext.columns[0] ?? "";
  if (fixX || fixY) {
    await call("setExternal", { label: ext.label, file: ext.file, text: ext.text, x: ext.x, y: ext.y });
  }
  renderExternal();
  invalidate("external_overlay");
}

function renderExternal() {
  const ext = state.external;
  $("#csv-fields").hidden = !ext;
  if (!ext) return;
  $("#csv-label").value = ext.label;
  for (const [sel, key] of [["#csv-x", "x"], ["#csv-y", "y"]]) {
    const s = $(sel);
    s.replaceChildren(...ext.columns.map((c) => new Option(c, c, false, c === ext[key])));
  }
}

// ---------------------------------------------------------------------------------------------
// Analyses
// ---------------------------------------------------------------------------------------------
const FILTER_PRESETS = [
  { name: "Pauses only", value: { op: "eq", column: "EventType", value: "Pause" } },
  { name: "Concurrent phases only", value: { op: "eq", column: "EventType", value: "Concurrent" } },
  { name: "Pauses or concurrent", value: { op: "in", column: "EventType", values: ["Concurrent", "Pause"] } },
  { name: "Has a duration", value: { op: "truthy", column: "Duration_milliseconds" } },
  { name: "Young collections", value: { op: "and", filters: [{ op: "eq", column: "EventType", value: "Pause" }, { op: "contains", column: "EventName", value: "Young" }] } },
  { name: "All events", value: { op: "all" } },
];

const GROUP_COLUMNS = ["EventType", "EventName", "AdditionalEventInfo", "Generation", "CodeHeap", "SafepointName", "GCIndex"];

function availableColumns() {
  const seen = new Set();
  for (const r of state.runs) for (const c of r.summary?.columns ?? []) if (c.non_null > 0) seen.add(c.name);
  const preferred = GROUP_COLUMNS.filter((c) => seen.has(c));
  const rest = [...seen].filter((c) => !preferred.includes(c)).sort();
  return [...preferred, ...rest];
}

function defaultParams(info) {
  const p = {};
  for (const s of info.params) p[s.key] = structuredClone(s.default);
  return p;
}

function buildCards() {
  const root = $("#analyses");
  const toc = $("#toc");
  root.replaceChildren();
  toc.replaceChildren();
  let section = null;
  const tpl = $("#card-template");
  for (const info of state.catalogue) {
    if (info.section !== section) {
      section = info.section;
      const h = document.createElement("h2");
      h.className = "section-title";
      h.textContent = section;
      root.append(h);
      const li = document.createElement("li");
      li.className = "section";
      li.textContent = section;
      toc.append(li);
    }
    state.params[info.id] = defaultParams(info);
    const card = tpl.content.firstElementChild.cloneNode(true);
    card.id = `a-${info.id}`;
    card.dataset.id = info.id;
    card.querySelector(".num").textContent = /^\d/.test(info.number) ? `#${info.number}` : info.number === "T" ? "★" : info.number;
    card.querySelector("h3").textContent = info.title;
    card.querySelector(".desc").textContent = info.description;
    const toggle = card.querySelector(".settings-toggle");
    const form = card.querySelector(".settings");
    if (!info.params.length) toggle.hidden = true;
    toggle.addEventListener("click", () => {
      form.hidden = !form.hidden;
      toggle.setAttribute("aria-expanded", String(!form.hidden));
      if (!form.hidden) buildSettings(info, form);
    });
    root.append(card);
    observer.observe(card);

    const li = document.createElement("li");
    const a = document.createElement("a");
    a.href = `#a-${info.id}`;
    a.innerHTML = `<span class="n"></span><span class="t"></span>`;
    a.querySelector(".n").textContent = info.number === "T" ? "★" : info.number;
    if (info.id === "custom") li.classList.add("custom");
    a.querySelector(".t").textContent = info.title;
    li.append(a);
    toc.append(li);
  }
}

function field(label, input, cls) {
  const l = document.createElement("label");
  if (cls) l.className = cls;
  if (input.type === "checkbox") {
    l.className = "check";
    l.append(input, document.createTextNode(label));
  } else {
    l.append(document.createTextNode(label), input);
  }
  return l;
}

function numberInput(value, step = "any") {
  const i = document.createElement("input");
  i.type = "number";
  i.step = step;
  i.value = value ?? "";
  return i;
}

function buildSettings(info, form) {
  form.replaceChildren();
  const params = state.params[info.id];
  const getters = [];
  for (const spec of info.params) {
    const key = spec.key;
    switch (spec.kind) {
      case "number": {
        const i = numberInput(params[key]);
        getters.push(() => (params[key] = i.value === "" ? spec.default : Number(i.value)));
        form.append(field(spec.label, i));
        break;
      }
      case "numbers": {
        const i = document.createElement("input");
        i.type = "text";
        i.value = (params[key] ?? []).join(", ");
        getters.push(() => {
          const v = i.value.split(/[\s,]+/).filter(Boolean).map(Number).filter((x) => Number.isFinite(x));
          params[key] = v.length ? v : spec.default;
        });
        form.append(field(spec.label, i));
        break;
      }
      case "select": {
        const s = document.createElement("select");
        for (const o of spec.options) s.append(new Option(o.replace(/_/g, " "), o));
        s.value = params[key] ?? spec.default;
        getters.push(() => (params[key] = s.value));
        form.append(field(spec.label, s));
        break;
      }
      case "bool": {
        const i = document.createElement("input");
        i.type = "checkbox";
        i.checked = !!params[key];
        getters.push(() => (params[key] = i.checked));
        form.append(field(spec.label, i));
        break;
      }
      case "column": {
        const s = document.createElement("select");
        s.append(new Option("(none)", ""));
        for (const c of availableColumns()) s.append(new Option(c, c));
        if (params[key] && ![...s.options].some((o) => o.value === params[key])) s.append(new Option(params[key], params[key]));
        s.value = params[key] ?? "";
        getters.push(() => (params[key] = s.value));
        form.append(field(spec.label, s));
        break;
      }
      case "filter": {
        const s = document.createElement("select");
        const current = JSON.stringify(params[key]);
        FILTER_PRESETS.forEach((p, idx) => s.append(new Option(p.name, String(idx))));
        s.append(new Option("Custom (JSON)…", "custom"));
        const match = FILTER_PRESETS.findIndex((p) => JSON.stringify(p.value) === current);
        s.value = match >= 0 ? String(match) : "custom";
        const ta = document.createElement("textarea");
        ta.value = JSON.stringify(params[key], null, 1);
        ta.spellcheck = false;
        const taLabel = field("Filter JSON (ops: eq, in, not_null, truthy, contains, and, or, not, all)", ta, "wide");
        taLabel.hidden = s.value !== "custom";
        s.addEventListener("change", () => {
          taLabel.hidden = s.value !== "custom";
          if (s.value !== "custom") ta.value = JSON.stringify(FILTER_PRESETS[Number(s.value)].value, null, 1);
        });
        getters.push(() => {
          if (s.value !== "custom") params[key] = structuredClone(FILTER_PRESETS[Number(s.value)].value);
          else params[key] = JSON.parse(ta.value);
        });
        form.append(field(spec.label, s), taLabel);
        break;
      }
      case "heatmap":
      case "log_heatmap": {
        const keys = spec.kind === "heatmap" ? ["x_buckets", "y_buckets", "x_seconds", "y_size"] : ["x_buckets", "y_buckets", "x_seconds", "base"];
        const wrap = document.createElement("span");
        wrap.className = "quad";
        const inputs = keys.map((k) => {
          const i = numberInput(params[key][k], k.endsWith("buckets") ? "1" : "any");
          i.title = k.replace("_", " ");
          i.setAttribute("aria-label", k.replace("_", " "));
          wrap.append(i);
          return i;
        });
        getters.push(() => {
          const v = {};
          keys.forEach((k, j) => (v[k] = Number(inputs[j].value)));
          params[key] = v;
        });
        form.append(field(spec.label, wrap));
        break;
      }
      case "grouping": {
        const s = document.createElement("select");
        for (const [v, name] of [["percent_of", "Percent of total"], ["sum", "Sum"], ["mean", "Mean"], ["max", "Max"], ["min", "Min"], ["count", "Count"], ["percentile", "Percentile"]]) {
          s.append(new Option(name, v));
        }
        s.value = params[key].fn;
        const n = numberInput(params[key].total ?? params[key].p ?? "");
        getters.push(() => {
          const fn = s.value;
          if (fn === "percent_of") params[key] = { fn, total: Number(n.value) || 8192 };
          else if (fn === "percentile") params[key] = { fn, p: Number(n.value) || 50 };
          else params[key] = { fn };
        });
        form.append(field(spec.label, s), field("Total / percentile", n));
        break;
      }
    }
  }
  const actions = document.createElement("span");
  actions.className = "actions";
  const reset = document.createElement("button");
  reset.type = "button";
  reset.className = "ghost small";
  reset.textContent = "Reset";
  reset.addEventListener("click", () => {
    state.params[info.id] = defaultParams(info);
    buildSettings(info, form);
    invalidate(info.id);
  });
  const apply = document.createElement("button");
  apply.type = "submit";
  apply.className = "small";
  apply.textContent = "Apply";
  actions.append(reset, apply);
  form.append(actions);
  form.onsubmit = (e) => {
    e.preventDefault();
    try {
      getters.forEach((g) => g());
    } catch (err) {
      showError(form.closest(".card"), `Invalid settings: ${err.message}`);
      return;
    }
    invalidate(info.id);
  };
}

// Render cards lazily as they scroll into view.
const observer = new IntersectionObserver(
  (entries) => {
    for (const e of entries) {
      const id = e.target.dataset.id;
      if (e.isIntersecting) {
        state.visible.add(id);
        if (state.stale.has(id)) renderCard(id);
      } else state.visible.delete(id);
    }
  },
  { rootMargin: "600px 0px" },
);

function invalidateAll() {
  state.generation++;
  for (const info of state.catalogue) invalidate(info.id);
}

function invalidate(id) {
  state.stale.add(id);
  if (state.visible.has(id)) renderCard(id);
}

function theme() {
  const cs = getComputedStyle(document.documentElement);
  const v = (n) => cs.getPropertyValue(n).trim();
  return { fg: v("--fg"), bg: v("--surface"), grid: v("--grid"), muted: v("--muted"), font: v("--font") };
}

function showError(card, msg) {
  const e = card.querySelector(".error");
  e.textContent = msg;
  e.hidden = !msg;
}

async function renderCard(id) {
  if (!state.runs.length || state.runs.some((r) => !r.summary)) return;
  state.stale.delete(id);
  const card = document.getElementById(`a-${id}`);
  const token = {};
  state.rendering.set(id, token);
  const figs = card.querySelector(".figures");
  if (!figs.children.length) {
    const ph = document.createElement("div");
    ph.className = "figure placeholder";
    ph.textContent = "Computing…";
    figs.append(ph);
  }
  let out;
  try {
    out = await call("analyse", { id, params: state.params[id] });
  } catch (err) {
    out = { error: err.message };
  }
  if (state.rendering.get(id) !== token) return; // superseded
  showError(card, out.error ? `Could not compute this analysis: ${out.error}` : "");
  const stdout = card.querySelector(".stdout");
  stdout.textContent = out.stdout ?? "";
  stdout.hidden = !out.stdout;
  const diag = card.querySelector(".diagnostics");
  diag.replaceChildren(...(out.diagnostics ?? []).map((d) => Object.assign(document.createElement("li"), { textContent: d })));
  diag.hidden = !(out.diagnostics ?? []).length;

  const figures = out.figures ?? [];
  for (const el of figs.querySelectorAll(".figure")) {
    if (window.Plotly && el.classList.contains("js-plotly-plot")) Plotly.purge(el);
  }
  figs.replaceChildren();
  figs.classList.toggle("many", figures.length > 1);
  if (!window.Plotly) await plotlyReady();
  const th = { ...theme(), narrow: figs.clientWidth < 640 };
  for (const fig of figures) {
    const el = document.createElement("div");
    el.className = "figure";
    figs.append(el);
    const { data, layout } = toPlotly(fig, th, { sparseHeatmap: id === "latency_heatmap" });
    if (!fig.axes.calls.length) {
      layout.annotations = [{ text: "No matching data in these runs", showarrow: false, xref: "paper", yref: "paper", x: 0.5, y: 0.5, font: { color: th.muted } }];
      layout.height = 260;
    }
    await Plotly.react(el, data, layout, {
      responsive: true,
      displaylogo: false,
      toImageButtonOptions: { filename: `gcgc_${id}`, scale: 2 },
      modeBarButtonsToRemove: ["select2d", "lasso2d"],
    });
  }
}

function plotlyReady() {
  return new Promise((resolve) => {
    const t = setInterval(() => {
      if (window.Plotly) {
        clearInterval(t);
        resolve();
      }
    }, 20);
  });
}

// ---------------------------------------------------------------------------------------------
// Wiring
// ---------------------------------------------------------------------------------------------
function wire() {
  const input = $("#file-input");
  const dz = $("#dropzone");
  input.addEventListener("change", () => {
    addRuns(groupFiles([...input.files]));
    input.value = "";
  });
  dz.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      input.click();
    }
  });
  for (const ev of ["dragenter", "dragover"]) {
    dz.addEventListener(ev, (e) => {
      e.preventDefault();
      dz.classList.add("over");
    });
  }
  for (const ev of ["dragleave", "drop"]) dz.addEventListener(ev, () => dz.classList.remove("over"));
  dz.addEventListener("drop", (e) => {
    e.preventDefault();
    addRuns(groupFiles([...e.dataTransfer.files]));
  });
  // Allow dropping anywhere on the page.
  document.addEventListener("dragover", (e) => e.preventDefault());
  document.addEventListener("drop", (e) => {
    if (e.target.closest && e.target.closest("#dropzone")) return;
    e.preventDefault();
    if (e.dataTransfer?.files?.length) addRuns(groupFiles([...e.dataTransfer.files]));
  });

  $("#examples").addEventListener("click", loadExamples);
  for (const id of ["#opt-zero", "#opt-crash"]) {
    $(id).addEventListener("change", () => state.runs.length && rebuildAllRuns());
  }

  $("#csv-input").addEventListener("change", async (e) => {
    const file = e.target.files[0];
    if (!file) return;
    state.external = { label: file.name.replace(/\.csv$/i, ""), file, columns: [], x: "", y: "" };
    try {
      await sendExternal();
    } catch (err) {
      state.external = null;
      renderExternal();
      setStatus(`CSV: ${err.message}`, true);
    }
  });
  const updateExternal = async () => {
    if (!state.external) return;
    state.external.label = $("#csv-label").value || "External data";
    state.external.x = $("#csv-x").value;
    state.external.y = $("#csv-y").value;
    await sendExternal();
  };
  for (const id of ["#csv-label", "#csv-x", "#csv-y"]) $(id).addEventListener("change", updateExternal);
  $("#csv-clear").addEventListener("click", async () => {
    state.external = null;
    $("#csv-input").value = "";
    await call("clearExternal");
    renderExternal();
    invalidate("external_overlay");
  });

  // Re-render charts when the colour scheme changes.
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => invalidateAll());
}

async function main() {
  wire();
  renderRuns();
  try {
    state.catalogue = await call("catalogue");
  } catch (err) {
    setStatus(`Engine failed to load: ${err.message}. Serve this folder over HTTP (see README).`, true);
    return;
  }
  buildCards();
  if (new URLSearchParams(location.search).has("examples")) loadExamples();
}

main();
