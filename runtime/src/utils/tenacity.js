// PythScribe Runtime — Utility Decorators
// Provides Pythonic decorator utilities for common patterns.

/**
 * Retry decorator — equivalent to tenacity's @retry.
 *
 * Usage in .ps files:
 *   @retry()
 *   @retry(max_attempts=3)
 *   @retry(max_attempts=5, delay=1.0, backoff="exponential")
 *   @retry(max_attempts=3, retry_on=TypeError)
 *
 * Options:
 *   max_attempts  — Maximum number of attempts (default: 3)
 *   delay         — Base delay between retries in seconds (default: 0)
 *   backoff       — "fixed" or "exponential" (default: "fixed")
 *   multiplier    — Multiplier for exponential backoff (default: 2)
 *   max_delay     — Maximum delay cap in seconds (default: 60)
 *   retry_on      — Error class or array of error classes to retry on (default: all errors)
 *   on_retry      — Callback(error, attempt) called before each retry
 *   reraise       — Re-throw the last error on failure (default: true)
 */
export function retry(options = {}) {
    const {
        max_attempts = 3,
        delay = 0,
        backoff = "fixed",
        multiplier = 2,
        max_delay = 60,
        retry_on = null,
        on_retry = null,
        reraise = true,
    } = options;

    function should_retry(error) {
        if (retry_on === null) return true;
        const types = Array.isArray(retry_on) ? retry_on : [retry_on];
        return types.some((t) => error instanceof t);
    }

    function compute_delay(attempt) {
        let d;
        if (backoff === "exponential") {
            d = delay * Math.pow(multiplier, attempt - 1);
        } else {
            d = delay;
        }
        return Math.min(d, max_delay);
    }

    function sleep(seconds) {
        return new Promise((resolve) => setTimeout(resolve, seconds * 1000));
    }

    return function decorator(fn) {
        // Detect if fn is async
        const is_async =
            fn.constructor.name === "AsyncFunction" ||
            fn[Symbol.toStringTag] === "AsyncFunction";

        if (is_async) {
            async function retried(...args) {
                let last_error;
                for (let attempt = 1; attempt <= max_attempts; attempt++) {
                    try {
                        return await fn(...args);
                    } catch (error) {
                        last_error = error;
                        if (attempt === max_attempts || !should_retry(error)) {
                            break;
                        }
                        if (on_retry) {
                            on_retry(error, attempt);
                        }
                        const wait = compute_delay(attempt);
                        if (wait > 0) {
                            await sleep(wait);
                        }
                    }
                }
                if (reraise) {
                    throw last_error;
                }
                return undefined;
            }
            Object.defineProperty(retried, "name", { value: fn.name });
            return retried;
        }

        // Sync version — returns a Promise if delay > 0, otherwise stays sync
        if (delay > 0) {
            // With delay, must be async
            async function retried_with_delay(...args) {
                let last_error;
                for (let attempt = 1; attempt <= max_attempts; attempt++) {
                    try {
                        return fn(...args);
                    } catch (error) {
                        last_error = error;
                        if (attempt === max_attempts || !should_retry(error)) {
                            break;
                        }
                        if (on_retry) {
                            on_retry(error, attempt);
                        }
                        const wait = compute_delay(attempt);
                        if (wait > 0) {
                            await sleep(wait);
                        }
                    }
                }
                if (reraise) {
                    throw last_error;
                }
                return undefined;
            }
            Object.defineProperty(retried_with_delay, "name", { value: fn.name });
            return retried_with_delay;
        }

        // Pure sync — no delay, immediate retries
        function retried_sync(...args) {
            let last_error;
            for (let attempt = 1; attempt <= max_attempts; attempt++) {
                try {
                    return fn(...args);
                } catch (error) {
                    last_error = error;
                    if (attempt === max_attempts || !should_retry(error)) {
                        break;
                    }
                    if (on_retry) {
                        on_retry(error, attempt);
                    }
                }
            }
            if (reraise) {
                throw last_error;
            }
            return undefined;
        }
        Object.defineProperty(retried_sync, "name", { value: fn.name });
        return retried_sync;
    };
}

/**
 * Convenience: stop_after_attempt — for tenacity-style usage.
 * Returns the max_attempts value for use with retry().
 */
export function stop_after_attempt(n) {
    return n;
}

/**
 * Convenience: wait_fixed — returns delay value in seconds.
 */
export function wait_fixed(seconds) {
    return seconds;
}

/**
 * Convenience: wait_exponential — returns config object.
 * Usage: @retry(wait=wait_exponential(multiplier=1, min=1, max=10))
 */
export function wait_exponential({ multiplier = 1, min = 0, max = 60 } = {}) {
    return { backoff: "exponential", delay: min, multiplier, max_delay: max };
}

/**
 * RetryError — thrown when all retry attempts are exhausted (if reraise=false).
 */
export class RetryError extends Error {
    constructor(last_error, attempts) {
        super(
            `RetryError: failed after ${attempts} attempts. Last error: ${last_error.message}`
        );
        this.name = "RetryError";
        this.last_error = last_error;
        this.attempts = attempts;
    }
}
