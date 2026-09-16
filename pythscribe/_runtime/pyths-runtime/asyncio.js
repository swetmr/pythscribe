// pyths.asyncio — Pythonic async helpers backed by JS Promises.
//
// Imported via: `from pyths.asyncio import gather, sleep, ...`
// Maps to standard JS Promise APIs at runtime.

/** Run multiple awaitables concurrently. Mirrors asyncio.gather, including
 *  `return_exceptions=True` (exceptions come back in-place in the result
 *  list instead of rejecting).
 *
 *  Kwargs plumbing: gather is variadic, so it carries no __pyparams__
 *  metadata and the compiler's __pyKwArgs helper appends the kwargs as a
 *  trailing plain object — pop it off here (Track-A sweep). */
export function gather(...awaitables) {
  let returnExceptions = false;
  const last = awaitables[awaitables.length - 1];
  if (
    last !== null &&
    typeof last === "object" &&
    !Array.isArray(last) &&
    typeof last.then !== "function" &&
    Object.prototype.hasOwnProperty.call(last, "return_exceptions")
  ) {
    for (const k of Object.keys(last)) {
      if (k !== "return_exceptions") {
        throw new TypeError(`gather() got an unexpected keyword argument '${k}'`);
      }
    }
    returnExceptions = !!last.return_exceptions;
    awaitables = awaitables.slice(0, -1);
  }
  if (!returnExceptions) return Promise.all(awaitables);
  return Promise.all(
    awaitables.map((a) => Promise.resolve(a).then((v) => v, (e) => e)),
  );
}

/** Sleep for `seconds` seconds. Mirrors asyncio.sleep. */
export function sleep(seconds) {
  return new Promise((resolve) => setTimeout(resolve, seconds * 1000));
}

/** Race multiple awaitables, return the first to resolve. */
export function wait_first(...awaitables) {
  return Promise.race(awaitables);
}

/** Resolve once all awaitables either resolve or reject (no throw). */
export function gather_settled(...awaitables) {
  return Promise.allSettled(awaitables);
}

/** Convert any value to a Promise. Mirrors asyncio.ensure_future. */
export function ensure_future(value) {
  return Promise.resolve(value);
}

/** Run a coroutine, return its result synchronously is impossible in JS;
 *  this helper just calls the function and returns the Promise. */
export function run(coro) {
  if (typeof coro === "function") {
    return Promise.resolve().then(() => coro());
  }
  return Promise.resolve(coro);
}

//# sourceMappingURL=asyncio.js.map
