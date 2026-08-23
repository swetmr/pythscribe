//! Bug batch (enriched-differential findings): end-to-end `pyths run` fences.
//!
//!   1. In-place augmented-assign protocol (aliasing soundness) — `x += [5]`
//!      / `d |= {...}` / set `|=`/`&=`/`-=`/`^=` must MUTATE the mutable
//!      target (identity preserved, aliases observe the change) like
//!      CPython's __iadd__/__ior__/... slots, while immutables (str/int/
//!      float/tuple) keep rebinding to a fresh object via the value helper.
//!   2. Negative shift count — `1 << -1` / `8 >> -2` raise
//!      ValueError("negative shift count") (old: silent wrong value).
//!   3. `0 ** -1` / `0.0 ** -1` raise ZeroDivisionError (old: OverflowError).
//!   4. `heapq.heappop([])` raises IndexError (old: silent None).

use std::process::Command;

fn pyths_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pyths"))
}

/// Compile-and-run `src` through `pyths run`, returning stdout.
fn run_ps(name: &str, src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("pyths_inplace_{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join(format!("{}.ps", name));
    std::fs::write(&ps, src).unwrap();
    let out = pyths_bin()
        .args(["run", ps.to_str().unwrap()])
        .output()
        .expect("pyths run failed to spawn");
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "pyths run {name} should exit 0; stderr={stderr}\nstdout={stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

#[test]
fn inplace_mutables_preserve_identity() {
    // The Bug-1 witnesses: list +=, dict |=, set |=/&=/-=/^=, list *= all
    // mutate in place — the alias sees the change and `is` stays True.
    let out = run_ps(
        "alias",
        r#"
x = [1, 2]
y = x
x += [5]
print(x is y, y)

d = {}
e = d
d |= {"a": 1}
print(d is e, e)

s = {1}
t = s
s |= {2}
print(s is t, sorted(s))

s2 = {1, 2, 3}
t2 = s2
s2 &= {2, 3, 4}
print(s2 is t2, sorted(s2))

s3 = {1, 2}
t3 = s3
s3 -= {2}
print(s3 is t3, sorted(s3))

s4 = {1, 2}
t4 = s4
s4 ^= {2, 3}
print(s4 is t4, sorted(s4))

m = [1, 2]
n = m
m *= 2
print(m is n, n)
m *= 0
print(m is n, n)
"#,
    );
    assert_eq!(
        out,
        "True [1, 2, 5]\n\
         True {'a': 1}\n\
         True [1, 2]\n\
         True [2, 3]\n\
         True [1]\n\
         True [1, 3]\n\
         True [1, 2, 1, 2]\n\
         True []\n"
    );
}

#[test]
fn inplace_immutables_still_rebind() {
    // Immutables have no in-place slot: `+=` falls back to the value helper
    // (a genuine rebind, fresh object) with unchanged results.
    let out = run_ps(
        "immut",
        r#"
a = "x"
a += "y"
print(a)
n = 1
n += 2
print(n)
tp = (1,)
tq = tp
tp += (2,)
print(tp, tq, tp is tq)
f = 1.5
f += 0.25
print(f)
big = 2 ** 62
big += 1
print(big)
"#,
    );
    assert_eq!(
        out,
        "xy\n3\n(1, 2) (1,) False\n1.75\n4611686018427387905\n"
    );
}

#[test]
fn inplace_subscript_target_mutates_stored_container() {
    // `d[k] += [..]` goes through the same in-place protocol: the STORED
    // list object is extended, so an outside alias observes the change.
    let out = run_ps(
        "subscript",
        r#"
inner = [1]
d = {"k": inner}
d["k"] += [2]
print(inner, d["k"] is inner)

xs = [[0]]
row = xs[0]
xs[0] += [9]
print(row, xs[0] is row)
"#,
    );
    assert_eq!(out, "[1, 2] True\n[0, 9] True\n");
}

#[test]
fn frozenset_augassign_rebinds_alias_safe() {
    // Reviewer's witness (frozenset aliasing-corruption regression): the
    // frozenset BRAND must make `|=`/`&=`/`-=`/`^=` REBIND via the value
    // helper — the alias keeps the original elements and `is` breaks —
    // exactly CPython. A plain `set` still mutates in place (Bug 1).
    let out = run_ps(
        "frozen",
        r#"
a = frozenset({1, 2, 3})
b = a
a &= {2, 3, 4}
print(sorted(b), a is b, sorted(a))

f1 = frozenset({1})
g1 = f1
f1 |= {2}
print(sorted(g1), f1 is g1, sorted(f1))

f2 = frozenset({1, 2})
g2 = f2
f2 -= {2}
print(sorted(g2), f2 is g2, sorted(f2))

f3 = frozenset({1, 2})
g3 = f3
f3 ^= {2, 3}
print(sorted(g3), f3 is g3, sorted(f3))

s = set({1})
t = s
s |= {2}
print(s is t, sorted(t))

# SECOND-OP chain (re-review blocker): the rebound result must itself be
# frozen (re-branded), so op #2 ALSO rebinds — an unbranded rebind would
# mutate in place on the second op and re-corrupt aliases.
a2 = frozenset({1})
a2 |= {2}
c2 = a2
a2 |= {3}
print(sorted(c2), a2 is c2, sorted(a2))

a3 = frozenset({1, 2, 3})
a3 &= {2, 3}
c3 = a3
a3 &= {3}
print(sorted(c3), a3 is c3, sorted(a3))

a4 = frozenset({1, 2, 3})
a4 -= {1}
c4 = a4
a4 -= {2}
print(sorted(c4), a4 is c4, sorted(a4))

a5 = frozenset({1})
a5 ^= {2}
c5 = a5
a5 ^= {1}
print(sorted(c5), a5 is c5, sorted(a5))
"#,
    );
    assert_eq!(
        out,
        "[1, 2, 3] False [2, 3]\n\
         [1] False [1, 2]\n\
         [1, 2] False [1]\n\
         [1, 2] False [1, 3]\n\
         True [1, 2]\n\
         [1, 2] False [1, 2, 3]\n\
         [2, 3] False [3]\n\
         [2, 3] False [3]\n\
         [1, 2] False [2]\n"
    );
}

#[test]
fn attribute_target_augassign_routes_through_helpers() {
    // Attribute targets were the ONE AugAssign form still on raw JS —
    // `c.items += [2]` string-coerced to "1,2". Now: same in-place
    // protocol (list mutates, aliases observe, identity preserved) and
    // value-helper semantics for numbers/strings.
    let out = run_ps(
        "attr",
        r#"
class C:
    def __init__(self):
        self.items = [1]
        self.n = 0
        self.s = "a"
        self.d = {"x": 1}

c = C()
d = c.items
c.items += [2]
print(c.items is d, d)
c.n += 1
c.n += 2 ** 62
print(c.n)
c.s += "b"
print(c.s)
e = c.d
c.d |= {"y": 2}
print(c.d is e, sorted(e))
"#,
    );
    assert_eq!(
        out,
        "True [1, 2]\n4611686018427387905\nab\nTrue ['x', 'y']\n"
    );
}

#[test]
fn attribute_target_receiver_evaluated_once() {
    // A side-effecting receiver (`getobj().attr += v`) must be evaluated
    // exactly once — the hoisted `__aug_o{n}` const, same as subscripts.
    let out = run_ps(
        "attr_once",
        r#"
class C:
    def __init__(self):
        self.xs = [1]

calls = []
c = C()

def getobj():
    calls.append(1)
    return c

getobj().xs += [2]
print(len(calls), c.xs)
"#,
    );
    assert_eq!(out, "1 [1, 2]\n");
}

#[test]
fn negative_shift_count_raises_value_error() {
    let out = run_ps(
        "shift",
        r#"
try:
    print(1 << -1)
except ValueError as e:
    print("L", e)
try:
    print(8 >> -2)
except ValueError as e:
    print("R", e)
print(1 << 3, 8 >> 2, 1 << 99)
"#,
    );
    assert_eq!(
        out,
        "L negative shift count\nR negative shift count\n8 2 633825300114114700748351602688\n"
    );
}

#[test]
fn zero_to_negative_power_raises_zero_division_error() {
    let out = run_ps(
        "pow",
        r#"
try:
    print(0 ** -1)
except ZeroDivisionError as e:
    print("I", e)
try:
    print(0.0 ** -1)
except ZeroDivisionError as e:
    print("F", e)
print(0 ** 0, 0 ** 2, 2 ** -1)
"#,
    );
    assert_eq!(
        out,
        "I 0.0 cannot be raised to a negative power\n\
         F 0.0 cannot be raised to a negative power\n\
         1 0 0.5\n"
    );
}

#[test]
fn heappop_empty_raises_index_error() {
    let out = run_ps(
        "heappop",
        r#"
import heapq

try:
    heapq.heappop([])
except IndexError as e:
    print("E", e)
h = []
heapq.heappush(h, 3)
heapq.heappush(h, 1)
print(heapq.heappop(h), heapq.heappop(h))
"#,
    );
    assert_eq!(out, "E index out of range\n1 3\n");
}
