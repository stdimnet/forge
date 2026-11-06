/**
 * `node scripts/cwm-yjs-bench.mjs`: the UI side of the Code With Me target "applying one
 * update to a 100 000-line file < 1 ms" (docs/plans/code-with-me.md, "Performance targets").
 * Runs in Node without a browser, with the UI's own `yjs` and `@codemirror/state`.
 *
 * A guest's editor holds a Y.Text of 100 000 lines bound to CodeMirror. For each remote
 * keystroke this measures what the UI does before the browser paints: `Y.applyUpdate` with
 * the text's observer turning the Yjs delta into CodeMirror changes (as y-codemirror.next
 * does), and the CodeMirror state transaction. Layout and painting are not included (they
 * depend on the viewport, not on the file's length). Measured on a fresh document, after
 * 20 000 characters typed in runs (as people type) and after 20 000 more single characters
 * at random places (the worst case: every one splits an item). Prints p50 / p99 / max per
 * phase and exits with 1 when a phase's p99 is 1 ms or more.
 *
 * `Y.applyUpdate` itself stays far below 1 ms; `YTextEvent.delta` walks the text's items
 * from the start, so its cost grows with the number of items before the change.
 */
import * as Y from "yjs";
import { EditorState } from "@codemirror/state";

const LINES = 100_000;
const SAMPLES = 2_000;

let seed = 7;
function rand(n) {
  seed ^= seed << 13;
  seed ^= seed >>> 17;
  seed ^= seed << 5;
  return (seed >>> 0) % n;
}

function stats(samples) {
  const s = [...samples].sort((a, b) => a - b);
  const at = (q) => s[Math.round((s.length - 1) * q)];
  return { p50: at(0.5), p99: at(0.99), max: s[s.length - 1] };
}

const fmt = ({ p50, p99, max }) => `p50 ${p50.toFixed(3)} ms, p99 ${p99.toFixed(3)} ms, max ${max.toFixed(3)} ms`;

let text = "";
for (let i = 0; i < LINES; i++) text += `    $order1->line(${i}, 'item-${String((i * 7919) % 100000).padStart(5, "0")}');\n`;

// The host's document, a typist (another participant) and this guest's editor.
const host = new Y.Doc();
host.getText("content").insert(0, text);
const typist = new Y.Doc();
Y.applyUpdate(typist, Y.encodeStateAsUpdate(host));
const guest = new Y.Doc();
Y.applyUpdate(guest, Y.encodeStateAsUpdate(host));
const gtext = guest.getText("content");
// A replica without observers: Yjs' own share of the work.
const bare = new Y.Doc();
Y.applyUpdate(bare, Y.encodeStateAsUpdate(host));
let state = EditorState.create({ doc: gtext.toString() });

// y-codemirror.next's observer: the delta becomes one CodeMirror transaction.
let pending = null;
gtext.observe((event) => {
  const changes = [];
  let pos = 0;
  for (const d of event.delta) {
    if (d.insert != null) {
      changes.push({ from: pos, to: pos, insert: d.insert });
    } else if (d.delete != null) {
      changes.push({ from: pos, to: pos + d.delete, insert: "" });
      pos += d.delete;
    } else {
      pos += d.retain;
    }
  }
  pending = changes;
});

function keystroke(doc, ch) {
  const t = doc.getText("content");
  const before = Y.encodeStateVector(doc);
  t.insert(rand(t.length + 1), ch);
  return Y.encodeStateAsUpdate(doc, before);
}

function measure(phase) {
  const plain = [];
  const yjs = [];
  const total = [];
  for (let i = 0; i < SAMPLES; i++) {
    const update = keystroke(typist, "z");
    Y.applyUpdate(host, update);
    const b0 = performance.now();
    Y.applyUpdate(bare, update, "remote");
    plain.push(performance.now() - b0);
    const t0 = performance.now();
    Y.applyUpdate(guest, update, "remote");
    const t1 = performance.now();
    state = state.update({ changes: pending }).state;
    const t2 = performance.now();
    yjs.push(t1 - t0);
    total.push(t2 - t0);
  }
  console.log(`${LINES} lines, ${phase}:`);
  console.log(`  Y.applyUpdate alone            ${fmt(stats(plain))}`);
  console.log(`  + the observer's event.delta   ${fmt(stats(yjs))}`);
  const all = stats(total);
  console.log(`  + the CodeMirror transaction   ${fmt(all)}`);
  return all;
}

// Warm up the JIT first, as a long-running editor is.
for (let i = 0; i < 500; i++) {
  const u = keystroke(typist, "w");
  Y.applyUpdate(host, u);
  Y.applyUpdate(guest, u, "remote");
  Y.applyUpdate(bare, u, "remote");
  state = state.update({ changes: pending }).state;
}

const others = [new Y.Doc(), new Y.Doc()];
for (const o of others) Y.applyUpdate(o, Y.encodeStateAsUpdate(host));
/** Another writer types `count` characters, `run` at a time in one place (one update each). */
function history(count, run) {
  for (let i = 0; i < count; i++) {
    const o = others[Math.floor(i / run) % 2];
    const t = o.getText("content");
    if (i % run === 0) o.at = rand(t.length + 1);
    const before = Y.encodeStateVector(o);
    t.insert(Math.min(o.at++, t.length), "y");
    const u = Y.encodeStateAsUpdate(o, before);
    for (const d of [host, typist, guest, bare, ...others]) if (d !== o) Y.applyUpdate(d, u, "remote");
    state = state.update({ changes: pending }).state;
  }
}

const results = [measure("fresh")];
history(20_000, 20);
results.push(measure("after 20 000 characters typed in runs of 20"));
history(20_000, 1);
results.push(measure("after another 20 000 single characters at random places (worst case)"));

if (state.doc.toString() !== gtext.toString() || gtext.toString() !== host.getText("content").toString()) {
  console.error("replicas diverged");
  process.exit(1);
}
const met = results.every((r) => r.p99 < 1);
console.log(met ? "target met: p99 < 1 ms" : "target missed: p99 >= 1 ms");
process.exit(met ? 0 : 1);
