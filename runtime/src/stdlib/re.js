// PythScribe standard library: re module
// Maps Python re functions to JavaScript RegExp

import { IndexError } from "../runtime.js";
import { pyRepr } from "../operators.js";

export const IGNORECASE = "i";
export const MULTILINE = "m";
export const DOTALL = "s";
export const GLOBAL = "g";
export const I = "i";
export const M = "m";
export const S = "s";

// Non-enumerable tuple marker (local twin of operators.js pyTuple, same
// pattern as collections.js) — subn returns a Python tuple `(result, n)`.
function __tup(items) {
    Object.defineProperty(items, "__pytuple__", { value: true, enumerable: false });
    return items;
}

function _buildFlags(flags) {
    if (!flags) return "";
    if (typeof flags === "string") return flags;
    return "";
}

// Translate Python-only regex syntax to its JS equivalent:
//   (?P<name>...)  ->  (?<name>...)     named group
//   (?P=name)      ->  \k<name>         named backreference
function _translatePattern(src) {
    return src
        .replace(/\(\?P</g, "(?<")
        .replace(/\(\?P=([A-Za-z_][A-Za-z0-9_]*)\)/g, "\\k<$1>");
}

function _toRegex(pattern, flags) {
    if (pattern instanceof Pattern) return pattern._regex;
    if (pattern instanceof RegExp) return pattern;
    // 'd' (hasIndices) so Match.start/end/span work for subgroups too.
    const f = _buildFlags(flags);
    return new RegExp(_translatePattern(String(pattern)), f.includes("d") ? f : f + "d");
}

// The pattern text a Match reports via `m.re.pattern` — the original
// string when the caller passed one, else the RegExp source.
function _patternString(pattern) {
    if (pattern instanceof Pattern) return pattern.pattern;
    return pattern instanceof RegExp ? pattern.source : String(pattern);
}

// Normalize any pattern argument to a Pattern object — what Match.re
// exposes and what compile() returns. Idempotent for Pattern inputs.
function _toPattern(pattern, flags) {
    if (pattern instanceof Pattern) return pattern;
    return new Pattern(_toRegex(pattern, flags), _patternString(pattern));
}

export class Match {
    constructor(m, string, pos, re) {
        this._match = m;
        this.string = string;
        this.pos = pos || 0;
        this.endpos = string.length;
        // Python's m.re is the compiled pattern object; expose the lite
        // shape user code actually reads (m.re.pattern).
        this.re = re !== undefined ? re : { pattern: undefined };
    }

    // Resolve a group index or name; IndexError on nonexistent groups
    // (CPython: "no such group"); undefined result = valid-but-unmatched.
    _resolve(group) {
        if (typeof group === "string") {
            if (!this._match.groups || !(group in this._match.groups)) {
                throw new IndexError("no such group");
            }
            return this._match.groups[group];
        }
        if (group < 0 || group >= this._match.length) {
            throw new IndexError("no such group");
        }
        return this._match[group];
    }

    group(...args) {
        if (args.length === 0) return this._match[0];
        if (args.length === 1) return this._resolve(args[0]) ?? null;
        return __tup(args.map(g => this._resolve(g) ?? null));
    }

    groups(dflt) {
        const result = [];
        for (let i = 1; i < this._match.length; i++) {
            result.push(this._match[i] !== undefined ? this._match[i] : (dflt !== undefined ? dflt : null));
        }
        return __tup(result);
    }

    groupdict(dflt) {
        const result = {};
        if (this._match.groups) {
            for (const [key, value] of Object.entries(this._match.groups)) {
                result[key] = value !== undefined ? value : (dflt !== undefined ? dflt : null);
            }
        }
        return result;
    }

    // [start, end] for a group. Group 0 comes from the match itself;
    // subgroups use the RegExp 'd'-flag indices (always set by _toRegex).
    // Valid-but-unmatched group -> [-1, -1], like CPython's span.
    _span(group) {
        if (group === 0) {
            return [this._match.index, this._match.index + this._match[0].length];
        }
        this._resolve(group); // IndexError on nonexistent group
        const ind = this._match.indices;
        if (!ind) return [-1, -1]; // caller-supplied RegExp without 'd'
        const sp = typeof group === "string" ? (ind.groups ? ind.groups[group] : undefined) : ind[group];
        return sp !== undefined ? [sp[0], sp[1]] : [-1, -1];
    }

    start(group = 0) { return this._span(group)[0]; }

    end(group = 0) { return this._span(group)[1]; }

    span(group = 0) { return __tup(this._span(group)); }

    // CPython: repr(m) == str(m) == "<re.Match object; span=(s, e), match='...'>"
    __repr__() {
        const [s, e] = this._span(0);
        return `<re.Match object; span=(${s}, ${e}), match=${pyRepr(this._match[0])}>`;
    }

    toString() { return this.__repr__(); }
}

export function search(pattern, string, flags) {
    const re = _toRegex(pattern, flags);
    const m = re.exec(string);
    return m ? new Match(m, string, 0, _toPattern(pattern, flags)) : null;
}

export function match(pattern, string, flags) {
    const re = _toRegex(pattern, flags);
    const src = re.source.startsWith("^") ? re.source : "^" + re.source;
    const m = new RegExp(src, re.flags).exec(string);
    return m ? new Match(m, string, 0, _toPattern(pattern, flags)) : null;
}

export function fullmatch(pattern, string, flags) {
    const re = _toRegex(pattern, flags);
    let src = re.source;
    if (!src.startsWith("^")) src = "^" + src;
    if (!src.endsWith("$")) src = src + "$";
    const m = new RegExp(src, re.flags).exec(string);
    return m ? new Match(m, string, 0, _toPattern(pattern, flags)) : null;
}

export function findall(pattern, string, flags) {
    const re = _toRegex(pattern, flags);
    const globalRe = new RegExp(re.source, re.flags.includes("g") ? re.flags : re.flags + "g");
    const results = [];
    let m;
    while ((m = globalRe.exec(string)) !== null) {
        if (m.length > 2) {
            results.push(__tup(m.slice(1).map(g => g !== undefined ? g : "")));
        } else if (m.length === 2) {
            results.push(m[1] !== undefined ? m[1] : "");
        } else {
            results.push(m[0]);
        }
        if (m[0].length === 0) globalRe.lastIndex++; // zero-width: avoid infinite loop
    }
    return results;
}

export function* finditer(pattern, string, flags) {
    const re = _toRegex(pattern, flags);
    const globalRe = new RegExp(re.source, re.flags.includes("g") ? re.flags : re.flags + "g");
    const patInfo = _toPattern(pattern, flags);
    let m;
    while ((m = globalRe.exec(string)) !== null) {
        yield new Match(m, string, 0, patInfo);
        if (m[0].length === 0) globalRe.lastIndex++;
    }
}

// Parse a Python replacement template (the `repl` argument of re.sub /
// re.subn) into segments, following CPython's sre_parse.parse_template:
//   \1 .. \99      numbered group backreference (at most 2 digits)
//   \0[0-7]{0,2}   octal character escape (\0 alone is NUL)
//   \[0-7]{3}      3 octal digits -> octal character escape
//   \g<name>       named group; \g<number> numbered group; \g<0> whole match
//   \\ \n \t \r \f \v \a \b   standard string escapes (\b is backspace)
//   \<ASCII letter not above>  -> "bad escape" error, like CPython
//   \<other char>  kept literally (backslash preserved)
// Previously the template was handed to JS String.replace verbatim, so
// Python backreferences came out literally ("\\3-\\2-\\1") and literal
// `$` collided with JS replacement-pattern syntax.
function _parseTemplate(repl) {
    const segs = [];
    let lit = "";
    const flushLit = () => { if (lit) { segs.push({ lit }); lit = ""; } };
    let i = 0;
    while (i < repl.length) {
        const c = repl[i];
        if (c !== "\\" || i + 1 >= repl.length) {
            lit += c;
            i++;
            continue;
        }
        const d = repl[i + 1];
        if (d >= "0" && d <= "9") {
            if (d === "0") {
                // Octal escape: \0 plus up to two more octal digits.
                let j = i + 2, oct = "0";
                while (j < repl.length && oct.length < 3 && repl[j] >= "0" && repl[j] <= "7") {
                    oct += repl[j]; j++;
                }
                lit += String.fromCharCode(parseInt(oct, 8));
                i = j;
                continue;
            }
            // Three octal digits -> octal escape; else up to 2 digits = group.
            const d2 = repl[i + 2], d3 = repl[i + 3];
            if (d >= "1" && d <= "7" &&
                d2 >= "0" && d2 <= "7" && d3 >= "0" && d3 <= "7") {
                lit += String.fromCharCode(parseInt(d + d2 + d3, 8) & 0xff);
                i += 4;
                continue;
            }
            let num = d, j = i + 2;
            if (j < repl.length && repl[j] >= "0" && repl[j] <= "9") { num += repl[j]; j++; }
            flushLit();
            segs.push({ idx: parseInt(num, 10) });
            i = j;
            continue;
        }
        if (d === "g") {
            if (repl[i + 2] !== "<") throw new Error("missing < in replacement template");
            const close = repl.indexOf(">", i + 3);
            if (close === -1) throw new Error("missing >, unterminated name in replacement template");
            const name = repl.slice(i + 3, close);
            if (name === "") throw new Error("missing group name in replacement template");
            flushLit();
            if (/^\d+$/.test(name)) segs.push({ idx: parseInt(name, 10) });
            else segs.push({ name });
            i = close + 1;
            continue;
        }
        if (d === "\\") { lit += "\\"; i += 2; continue; }
        const esc = { n: "\n", t: "\t", r: "\r", f: "\f", v: "\v", a: "\x07", b: "\b" };
        if (esc[d] !== undefined) { lit += esc[d]; i += 2; continue; }
        if (/[A-Za-z]/.test(d)) throw new Error(`bad escape \\${d} at position ${i}`);
        lit += "\\" + d;
        i += 2;
    }
    flushLit();
    return segs;
}

// Expand a parsed template against a JS RegExp match array. Unmatched
// (but valid) groups substitute the empty string, as CPython >= 3.5 does;
// references to nonexistent groups raise like CPython's re.error.
function _expandTemplate(segs, m) {
    let out = "";
    for (const s of segs) {
        if (s.lit !== undefined) {
            out += s.lit;
        } else if (s.idx !== undefined) {
            if (s.idx >= m.length) throw new Error(`invalid group reference ${s.idx}`);
            out += m[s.idx] !== undefined ? m[s.idx] : "";
        } else {
            if (!m.groups || !(s.name in m.groups)) throw new Error(`unknown group name '${s.name}'`);
            out += m.groups[s.name] !== undefined ? m.groups[s.name] : "";
        }
    }
    return out;
}

// Shared engine for sub/subn: manual global-exec loop (rather than
// String.replace) so Python template semantics, count limiting, and
// zero-width-match advancement all live on one code path.
function _subImpl(pattern, repl, string, count, flags) {
    const re = _toRegex(pattern, flags);
    const globalRe = new RegExp(re.source, re.flags.includes("g") ? re.flags : re.flags + "g");
    const segs = typeof repl === "function" ? null : _parseTemplate(String(repl));
    const patInfo = _toPattern(pattern, flags);
    let out = "";
    let last = 0;
    let n = 0;
    let m;
    while ((count === 0 || n < count) && (m = globalRe.exec(string)) !== null) {
        out += string.slice(last, m.index);
        out += segs === null
            ? String(repl(new Match(m, string, 0, patInfo)))
            : _expandTemplate(segs, m);
        last = m.index + m[0].length;
        n++;
        if (m[0].length === 0) globalRe.lastIndex++;
    }
    out += string.slice(last);
    return [out, n];
}

export function sub(pattern, repl, string, count = 0, flags) {
    return _subImpl(pattern, repl, string, count, flags)[0];
}

export function subn(pattern, repl, string, count = 0, flags) {
    return __tup(_subImpl(pattern, repl, string, count, flags));
}

export function split(pattern, string, maxsplit = 0, flags) {
    const re = _toRegex(pattern, flags);
    if (maxsplit === 0) return string.split(re);
    const parts = [];
    let remaining = string;
    for (let i = 0; i < maxsplit; i++) {
        const m = re.exec(remaining);
        if (!m) break;
        parts.push(remaining.slice(0, m.index));
        remaining = remaining.slice(m.index + m[0].length);
    }
    parts.push(remaining);
    return parts;
}

// Python re.Pattern: what re.compile() returns and what Match.re exposes.
// Previously compile() returned a plain object that module-level functions
// did NOT accept back (re.match(re.compile(p), s) matched nothing because
// _toRegex stringified the object) — BigCodeBench/1108.
export class Pattern {
    constructor(regex, pattern) {
        this._regex = regex;         // underlying JS RegExp
        this.pattern = pattern;      // original Python pattern text
        this.flags = regex.flags;
    }

    search(string) { return search(this, string); }
    match(string) { return match(this, string); }
    fullmatch(string) { return fullmatch(this, string); }
    findall(string) { return findall(this, string); }
    finditer(string) { return finditer(this, string); }
    sub(repl, string, count = 0) { return sub(this, repl, string, count); }
    subn(repl, string, count = 0) { return subn(this, repl, string, count); }
    split(string, maxsplit = 0) { return split(this, string, maxsplit); }

    __repr__() { return `re.compile(${pyRepr(this.pattern)})`; }
    toString() { return this.__repr__(); }
}

export function compile(pattern, flags) {
    return _toPattern(pattern, flags);
}

export function escape(string) {
    return string.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

//# sourceMappingURL=re.js.map
