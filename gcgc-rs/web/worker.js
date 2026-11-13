// Web Worker hosting the WebAssembly analysis engine, so parsing large logs never blocks the page.
// Protocol: postMessage({id, cmd, args}) -> {id, ok: true, result} | {id, ok: false, error}
import init, { Session } from "./pkg/gcgc_wasm.js";

let session = null;
const ready = init().then(() => {
  session = new Session();
});

const commands = {
  async addRun({ label, files, zeroTimes, ignoreCrashes }) {
    const names = [];
    const texts = [];
    for (const f of files) {
      names.push(f.name);
      texts.push(typeof f.text === "string" ? f.text : await f.file.text());
    }
    return JSON.parse(session.addRun(label, names, texts, zeroTimes, ignoreCrashes));
  },
  removeRun: ({ index }) => session.removeRun(index),
  renameRun: ({ index, label }) => session.renameRun(index, label),
  moveRun: ({ from, to }) => session.moveRun(from, to),
  clear: () => session.clear(),
  async setExternal({ label, file, text, x, y }) {
    const csv = text ?? (await file.text());
    return JSON.parse(session.setExternal(label, csv, x, y));
  },
  clearExternal: () => session.clearExternal(),
  catalogue: () => JSON.parse(Session.catalogue()),
  analyse: ({ id, params }) => JSON.parse(session.analyse(id, params ? JSON.stringify(params) : "")),
  exportCsv: ({ index }) => session.exportCsv(index),
};

self.onmessage = async (e) => {
  const { id, cmd, args } = e.data;
  try {
    await ready;
    const fn = commands[cmd];
    if (!fn) throw new Error(`unknown command ${cmd}`);
    const result = await fn(args || {});
    self.postMessage({ id, ok: true, result });
  } catch (err) {
    self.postMessage({ id, ok: false, error: String(err && err.message ? err.message : err) });
  }
};
