//! Behavioral differential — the CPython differential expressed as a `cargo test`,
//! so `cargo-mutants` uses OBSERVABLE BEHAVIOR (not emitted-JS shape) as the kill
//! criterion. Each corpus program is compiled via the (possibly-mutated) codegen
//! library, run under Node, and its stdout compared to the CPython golden. A
//! mutation to `emit.rs` that changes behavior on the corpus fails here -> caught.
//! Corpus + goldens AUTO-GENERATED from live CPython (experiments/.../gen_corpus.py).

use std::process::Command;

// AUTO-GENERATED corpus (CPython golden). (name, source, expected_stdout)
const CORPUS: &[(&str, &str, &str)] = &[
    ("arith_fdiv", "print(-7 // 2, 7 % -3, 2 ** 10, 17 // 5, -17 % 5)", "-4 -2 1024 3 3\n"),
    ("float_ops", "print(10 / 3, 3.14 * 2, 1.0 == 1)", "3.3333333333333335 6.28 True\n"),
    ("neg_index", "xs=[1,2,3,4]\nprint(xs[-1], xs[-2])", "4 3\n"),
    ("slices", "xs=[0,1,2,3,4,5]\nprint(xs[1:4], xs[::2], xs[::-1], xs[-2:])", "[1, 2, 3] [0, 2, 4] [5, 4, 3, 2, 1, 0] [4, 5]\n"),
    ("list_dict_set", "print([1,2,3], {'a':1,'b':2}, sorted({3,1,2}))", "[1, 2, 3] {'a': 1, 'b': 2} [1, 2, 3]\n"),
    ("comp_map_filter", "print([x*x for x in range(5)], [x for x in range(10) if x%2==0])", "[0, 1, 4, 9, 16] [0, 2, 4, 6, 8]\n"),
    ("dict_comp", "print({k: k*2 for k in range(3)})", "{0: 0, 1: 2, 2: 4}\n"),
    ("nested_comp", "print([[i*j for j in range(3)] for i in range(3)])", "[[0, 0, 0], [0, 1, 2], [0, 2, 4]]\n"),
    ("control_flow", "s=0\nfor i in range(5):\n    if i%2: s+=i\n    else: s-=i\nprint(s)", "-2\n"),
    ("while_loop", "n=10\nc=0\nwhile n>1:\n    n = n//2 if n%2==0 else 3*n+1\n    c+=1\nprint(c)", "6\n"),
    ("functions", "def f(a, b=2, *rest):\n    return a + b + sum(rest)\nprint(f(1), f(1, 3, 4, 5))", "3 13\n"),
    ("recursion", "def fib(n):\n    return n if n<2 else fib(n-1)+fib(n-2)\nprint(fib(15))", "610\n"),
    ("closures", "def mk(n):\n    def add(x): return x+n\n    return add\nprint(mk(10)(5), mk(100)(5))", "15 105\n"),
    ("fstrings", "x=42\ny=3.14159\nprint(f'{x} {y:.2f} {x:04d} {x!r}')", "42 3.14 0042 42\n"),
    ("str_methods", "s='Hello World'\nprint(s.upper(), s.lower(), s.split(), s.replace('o','0'), len(s))", "HELLO WORLD hello world ['Hello', 'World'] Hell0 W0rld 11\n"),
    ("str_slice_neg", "s='python'\nprint(s[-1], s[::-1], s[1:4])", "n nohtyp yth\n"),
    ("bool_truthy", "print(bool([]), bool([0]), bool(''), bool('x'), bool(0), bool({}))", "False True False True False False\n"),
    ("tuples_unpack", "a,b,c=1,2,3\n(x,y),z=(10,20),30\nprint(a,b,c,x,y,z)", "1 2 3 10 20 30\n"),
    ("ternary_shortcircuit", "print(1 if True else 2, [] or 'd', 5 and 6, None or 0 or 'last')", "1 d 6 last\n"),
    ("aug_assign", "x=10\nx+=5\nx*=2\nx//=3\nx%=7\nprint(x)", "3\n"),
    ("builtins", "print(len([1,2,3]), sum(range(5)), max(3,7,2), min([4,1,9]), abs(-5))", "3 10 7 1 5\n"),
    ("enumerate_zip", "print(list(enumerate(['a','b'])), list(zip([1,2],[3,4])))", "[(0, 'a'), (1, 'b')] [(1, 3), (2, 4)]\n"),
    ("sorted_key", "xs=[(1,'b'),(2,'a'),(0,'c')]\nprint(sorted(xs, key=lambda t:t[1]))", "[(2, 'a'), (1, 'b'), (0, 'c')]\n"),
    ("try_except", "def g(x):\n    try:\n        return 10//x\n    except ZeroDivisionError:\n        return -1\nprint(g(2), g(0))", "5 -1\n"),
    ("classes", "class P:\n    def __init__(self,x): self.x=x\n    def dbl(self): return self.x*2\np=P(21)\nprint(p.x, p.dbl())", "21 42\n"),
    ("walrus", "xs=[1,2,3,4,5]\nprint([y for x in xs if (y:=x*x)>5])", "[9, 16, 25]\n"),
    ("dict_methods", "d={'a':1,'b':2}\nprint(list(d.keys()), list(d.values()), d.get('c',99), 'a' in d)", "['a', 'b'] [1, 2] 99 True\n"),
    ("string_join_fmt", "print(', '.join(str(i) for i in range(4)), '-'.join(['a','b','c']))", "0, 1, 2, 3 a-b-c\n"),
    ("int_bignum", "print(2**64, 10**20 + 1, (2**63) - 1)", "18446744073709551616 100000000000000000001 9223372036854775807\n"),
    ("nested_data", "d={'list':[1,2,{'k':3}],'t':(4,5)}\nprint(d['list'][2]['k'], d['t'][0])", "3 4\n"),
    // P2-corpus: genuine corpus-gaps from Track A survivor clustering (§20.6) -
    // range step-sign guards (try_emit_range_for L7737-7738), kwargs count, dict-comp conds.
    ("range_neg_step", "r=[]\nfor i in range(10,0,-2): r.append(i)\nprint(r)", "[10, 8, 6, 4, 2]\n"),
    ("range_neg_step1", "s=0\nfor i in range(5,0,-1): s=s*10+i\nprint(s)", "54321\n"),
    ("range_pos_step", "r=[]\nfor i in range(0,10,3): r.append(i)\nprint(r)", "[0, 3, 6, 9]\n"),
    ("range_empty", "c=0\nfor i in range(5,5): c+=1\nfor i in range(0): c+=1\nprint(c)", "0\n"),
    ("range_neg_var", "n=-3\nr=[]\nfor i in range(20,5,n): r.append(i)\nprint(r)", "[20, 17, 14, 11, 8]\n"),
    ("range_for_else", "hit=0\nfor i in range(8,2,-2):\n    if i==0: break\nelse:\n    hit=1\nprint(hit)", "1\n"),
    ("kwargs_count", "def f(**kw): return sum(kw.values())\nprint(f(), f(a=1), f(a=1,b=2,c=3))", "0 1 6\n"),
    ("dictcomp_cond", "print({k:v for k,v in [(1,'a'),(2,'b'),(3,'c')] if k!=2})", "{1: 'a', 3: 'c'}\n"),
    ("dictcomp_nested", "print({i:{j:i*j for j in range(2)} for i in range(3)})", "{0: {0: 0, 1: 0}, 1: {0: 0, 1: 1}, 2: {0: 0, 1: 2}}\n"),
    // #284 collections.ChainMap. Order note: CPython builds its key view from
    // reversed(maps), so dict()/keys() list the right-most map's keys first
    // while values obey first-map-wins precedence (goldens from CPython 3.12).
    ("chainmap_dict_merge", "from collections import ChainMap\nprint(dict(ChainMap({'a': 1}, {'b': 2})))", "{'b': 2, 'a': 1}\n"),
    ("chainmap_precedence", "from collections import ChainMap\ncm = ChainMap({'a': 1}, {'a': 2, 'b': 3})\nprint(cm['a'], cm['b'], list(cm.keys()))", "1 3 ['a', 'b']\n"),
    ("chainmap_views_repr", "from collections import ChainMap\ncm = ChainMap({'a': 1}, {'a': 2, 'b': 3})\nprint(list(cm.values()), len(cm), 'a' in cm, 'z' in cm, cm.get('z', -1))\nprint(cm)", "[1, 3] 2 True False -1\nChainMap({'a': 1}, {'a': 2, 'b': 3})\n"),
    ("chainmap_new_child", "from collections import ChainMap\ncm = ChainMap({'a': 1}, {'b': 2})\nc = cm.new_child({'x': 9})\nprint(c['x'], c['a'], len(c), list(c.keys()))", "9 1 3 ['b', 'a', 'x']\n"),
    // #283: complex literals + basic complex arithmetic (parse + runtime). cmath
    // and complex `/`,`**` are out of scope. Goldens from live CPython.
    ("complex_abs", "print(abs(3 + 4j))", "5.0\n"),
    ("complex_ops", "print(2j, 1 + 2j, (1+2j)*(3+4j))", "2j (1+2j) (-5+10j)\n"),
    ("complex_attrs", "print((2j).real, (3+4j).imag)", "0.0 4.0\n"),
    ("complex_neg_sub", "print(1 - 2j, -(3+4j), 2j - 5)", "(1-2j) (-3-4j) (-5+2j)\n"),
    // SECURITY regression (injection cluster): a string whose content contains
    // the JS string-literal breakout sequence `";` must be escaped, not spliced
    // — it prints as data, and no injected statement runs. Guards escape_js_string.
    ("str_quote_breakout", "print('a\";globalThis.PWNED=1;//')", "a\";globalThis.PWNED=1;//\n"),
    ("str_backslash_quote", "print('a\\\\b', 'c\\\"d')", "a\\b c\"d\n"),
    // B3 (diamond MRO): C3 order is [D, B, C, A]; `who` must resolve to C's
    // genuine override, not B's flattened copy of A.who. Cooperative super()
    // must reach C's branch, not stop at B's inherited copy. CPython goldens.
    ("diamond_mro", "class A:\n    def who(self): return 'A'\nclass B(A): pass\nclass C(A):\n    def who(self): return 'C'\nclass D(B, C): pass\nprint(D().who())", "C\n"),
    ("diamond_super", "class A:\n    def who(self): return 'A'\nclass B(A): pass\nclass C(A):\n    def who(self): return 'C'\nclass D(B, C):\n    def who(self): return 'D' + super().who()\nprint(D().who())", "DC\n"),
    // B4: list(d)/tuple(d) on an all-string-key dict (plain-object shape) must
    // yield the KEYS; list() must return an independent copy.
    ("list_tuple_dict_keys", "d={'b':1,'a':2}\nprint(list(d), tuple(d), sorted(d))", "['b', 'a'] ('b', 'a') ['a', 'b']\n"),
    ("list_copy_semantics", "xs=[1,2,3]\nys=list(xs)\nys.append(4)\nprint(xs, ys)", "[1, 2, 3] [1, 2, 3, 4]\n"),
    // B6: str.format() honors format specs, brace escaping, and named fields.
    ("str_format_specs", "print('{}-{:03d}'.format('x', 7), '{0:>5}|{1:.2f}'.format('hi', 3.14159))", "x-007    hi|3.14\n"),
    ("str_format_escape_named", "print('{{literal}} {name:>6}'.format(name='hi'))", "{literal}     hi\n"),
];

#[test]
fn behavioral_differential_matches_cpython() {
    let dir = std::env::temp_dir().join(format!("pyths_behdiff_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    pyths_runtime::materialize_runtime_package(&dir).expect("materialize runtime");

    let mut failures = Vec::new();
    for (name, src, expected) in CORPUS {
        let module = match pyths_parser::parse(src) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("{name}: PARSE FAILED {e:?}"));
                continue;
            }
        };
        let js = pyths_codegen_js::codegen_inline(&module);
        let path = dir.join(format!("{name}.mjs"));
        std::fs::write(&path, &js).unwrap();
        let out = Command::new("node")
            .arg(&path)
            .output()
            .expect("node available");
        let got = String::from_utf8_lossy(&out.stdout);
        if got != *expected {
            let err = String::from_utf8_lossy(&out.stderr);
            failures.push(format!(
                "{name}: got {:?} want {:?} (rc {:?}; {})",
                got,
                expected,
                out.status.code(),
                err.lines().last().unwrap_or("")
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        failures.is_empty(),
        "{} behavioral difference(s):
{}",
        failures.len(),
        failures.join(
            "
"
        )
    );
}
