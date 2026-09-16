// wasm_net: per-call watchdog shared by the node arms. A compiled call that never
// returns (e.g. a `range(a, b, 0)` loop the WASM lowering spins on where CPython raises
// ValueError) must become a RECORDED outcome — `{kind: "hang"}` — not a stuck harness.
// The calls run in a worker thread that posts each outcome as it completes; the main
// thread terminates the worker when one call exceeds the budget and resumes from the
// next call in a fresh worker (a fresh module instance, so a hung call cannot poison
// the state of the ones after it).
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

export const CALL_TIMEOUT_MS = Number(process.env.WASM_NET_CALL_TIMEOUT_MS || 4000);

export { isMainThread };

/** Flatten the spec into an ordered call plan. */
export function planOf(spec) {
  const plan = [];
  for (const [name, f] of Object.entries(spec.functions)) f.calls.forEach((_, i) => plan.push([name, i]));
  return plan;
}

/** Main thread: drive the plan through (re-spawned) workers; returns {name: [outcome...]}. */
export async function orchestrate(spec, selfUrl, extraData) {
  const plan = planOf(spec);
  const results = {};
  for (const [name, f] of Object.entries(spec.functions)) results[name] = new Array(f.calls.length).fill(null);
  let pos = 0;
  while (pos < plan.length) {
    pos = await new Promise((resolve) => {
      const start = pos;
      let current = start;
      let timer = null;
      let settled = false; // the segment's verdict is decided exactly once
      const w = new Worker(new URL(selfUrl), { workerData: { ...extraData, start } });
      const arm = () => {
        clearTimeout(timer);
        timer = setTimeout(() => {
          if (settled) return;
          settled = true;
          const [name, i] = plan[current];
          results[name][i] = { kind: "hang", exc: `no return within ${CALL_TIMEOUT_MS}ms` };
          w.terminate().then(() => resolve(current + 1));
        }, CALL_TIMEOUT_MS);
      };
      arm();
      w.on("message", (m) => {
        const [name, i] = plan[m.idx];
        results[name][i] = m.outcome;
        current = m.idx + 1;
        if (current >= plan.length) { clearTimeout(timer); return; }
        arm();
      });
      w.on("error", (e) => {
        clearTimeout(timer);
        if (settled) return;
        settled = true;
        const [name, i] = plan[current];
        results[name][i] = { kind: "err", exc: "WorkerError:" + String(e).slice(0, 100) };
        resolve(current + 1);
      });
      w.on("exit", () => {
        clearTimeout(timer);
        if (settled) return; // a timeout / error already decided where to resume
        settled = true;
        if (current < plan.length) {
          // The worker died without posting an outcome for `current` (an import or
          // instantiation failure): record it and move on so the segment cannot loop.
          const [name, i] = plan[current];
          results[name][i] = { kind: "err", exc: "WorkerExit: worker exited without an outcome" };
          resolve(current + 1);
        } else {
          resolve(plan.length);
        }
      });
    });
  }
  return results;
}

/** Worker thread: execute plan[start..] with `exec(name, i)` and post each outcome. */
export function serve(spec, exec) {
  const plan = planOf(spec);
  for (let idx = workerData.start; idx < plan.length; idx++) {
    const [name, i] = plan[idx];
    let outcome;
    try {
      outcome = exec(name, i);
    } catch (e) {
      outcome = { kind: "err", exc: "HarnessError:" + String(e && e.stack ? e.stack.split("\n")[0] : e).slice(0, 120) };
    }
    parentPort.postMessage({ idx, outcome });
  }
}
