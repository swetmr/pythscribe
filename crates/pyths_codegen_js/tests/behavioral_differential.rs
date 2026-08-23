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
    // E2-lite operand-type authority (#466 class): incompatible binary-op
    // operand pairs raise CPython's exact TypeError KIND + MESSAGE through
    // __binOpTypeError instead of silently falling through to a raw JS
    // operator (base: b'abc' + 1 was the silent string "97,98,991",
    // 'ab' * 2.5 the silent "abab", 'ab' * 'c' a raw SyntaxError). Valid
    // replications/concats — bool counts, negative counts, tuple-ness —
    // stay CPython-exact. Goldens from live CPython 3.12.
    ("operand_add_typeerrors", "def m(f):\n    try:\n        f()\n        return 'no-throw'\n    except TypeError as e:\n        return str(e)\nprint(m(lambda: b'abc' + 1))\nprint(m(lambda: 'a' + 1))\nprint(m(lambda: [1] + 1))\nprint(m(lambda: 1 + 'a'))", "can't concat int to bytes\ncan only concatenate str (not \"int\") to str\ncan only concatenate list (not \"int\") to list\nunsupported operand type(s) for +: 'int' and 'str'\n"),
    ("operand_mul_typeerrors", "def m(f):\n    try:\n        f()\n        return 'no-throw'\n    except TypeError as e:\n        return str(e)\nprint(m(lambda: b'ab' * 2.5))\nprint(m(lambda: 'ab' * 'c'))\nprint(m(lambda: [1] * 2.5))\nprint(m(lambda: 2.5 * 'ab'))", "can't multiply sequence by non-int of type 'float'\ncan't multiply sequence by non-int of type 'str'\ncan't multiply sequence by non-int of type 'float'\ncan't multiply sequence by non-int of type 'float'\n"),
    ("operand_valid_replication", "print('ab' * True, repr('ab' * -1), b'ab' * True, [1, 2] * 2, (1, 2) * 2, True * 2)", "ab '' b'ab' [1, 2, 1, 2] (1, 2, 1, 2) 2\n"),
    // Option B (#451 minimal): integer-valued floats carry a PyFloat box, so
    // 8.0 vs 8 stays distinguishable in CONTAINERS and dynamic contexts —
    // the class of repr bug the old Number.isInteger-everywhere heuristic
    // could not fix. CPython goldens; salvaged/adapted from the Option A
    // float-repr rows (int reprs are back to native: 5, not 5n).
    ("float_container_repr", "print([8.0, 9.0], [0.0, 1.0], [1, 2.0, 3], [float(5)])", "[8.0, 9.0] [0.0, 1.0] [1, 2.0, 3] [5.0]\n"),
    ("float_scalar_tuple_repr", "print(8.0, (3.9, 4.0), {1.0: 'a', 2: 'b'})", "8.0 (3.9, 4.0) {1.0: 'a', 2: 'b'}\n"),
    ("float_arith_repr", "print(1.5 + 2.5, 7 / 2, 8 / 2, 2.0 ** 3, 9.0 // 2.0, 10.0 % 5.0)", "4.0 3.5 4.0 8.0 4.0 0.0\n"),
    ("float_int_cross", "print(8.0 == 8, 8.0 in [8], (8.0,) == (8,), {8.0: 'x'}[8], -8.0, abs(-8.0))", "True True True x -8.0 8.0\n"),
    ("float_fn_results", "import math\nprint(math.sqrt(16), round(3.0, 1), sum([1.0, 2.0]), float('nan'), 1e300)", "4.0 3.0 3.0 nan 1e+300\n"),
    ("float_fstring", "print(f'{8.0}|{8.0:>6}|{8.0:.2f}|{0.0}')", "8.0|   8.0|8.00|0.0\n"),
    ("float_truthiness", "print(bool(0.0), bool(8.0), 'y' if 0.0 else 'n')", "False True n\n"),
    ("float_json", "import json\nprint(json.dumps({'a': 2.0, 'b': 3.14, 'c': 5}), json.dumps([1.0, 2.5]))", "{\"a\": 2.0, \"b\": 3.14, \"c\": 5} [1.0, 2.5]\n"),
    ("float_isinstance_type", "print(isinstance(8.0, float), isinstance(8.0, int), isinstance(8, int), type(8.0) is float)", "True False True True\n"),
    ("int_untouched_exactness", "print(2**53 + 1, 2**53 - 1, 5, [1, 2, 3])", "9007199254740993 9007199254740991 5 [1, 2, 3]\n"),
    // Review blocker: boxed integer-valued floats must coerce through
    // __toComplex (THE complex-coercion authority) — complex(8.0) threw and
    // abs(3.0+4.0j) was silent nan before the fix. Complex .real/.imag and
    // abs(complex) are FLOATS in CPython (re-tagged at the read surface).
    ("complex_boxed_float", "print(complex(8.0), complex(8.0, 2.0), 8.0 + 2j, 8.0 * 2j)", "(8+0j) (8+2j) (8+2j) 16j\n"),
    ("complex_attrs_abs", "z = 8.0 + 0j\nprint(z.real, z.imag, abs(3.0 + 4.0j), [abs(3.0 + 4.0j)])", "8.0 0.0 5.0 [5.0]\n"),
    // Review should-fix: random.Random INSTANCE methods carry the float tag
    // like the module-level fns (uniform(5,5) is 5.0, not int 5).
    ("random_instance_float", "import random\nr = random.Random(0)\nprint(r.uniform(5, 5), [r.uniform(2, 2)], r.randint(1, 1))", "5.0 [2.0] 1\n"),
    // Option A blocker regression rows, salvaged TRIMMED to the int-ness
    // claims (a bytes element is a native-Number int under B — no change
    // needed). The dropped sub-parts (bytes negative index, subsequence
    // membership, .index/.count, bytearray slice type) depended on Option
    // A's separate bytes-query-engine fixes (149529aa/12713d6b), which the
    // B mandate explicitly does not carry; those remain pre-existing base
    // gaps, tracked for their own fix batch.
    ("bytes_elem_int", "b = b\"abc\"\nprint(b[0], type(b[0]).__name__, b[0] + 1)", "97 int 98\n"),
    ("bytes_iter_sum_member", "print(list(b\"AB\"), sum(b\"abc\"), 65 in b\"ABC\", 68 in b\"ABC\")", "[65, 66] 294 True False\n"),
    ("bytearray_elem_write", "ba = bytearray(b\"xyz\")\nba[1] = 65\nprint(ba[1], ba)", "65 bytearray(b'xAz')\n"),
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
    // 0.2.2 conformance review FIX 1 (INLINE-path regression guard): bound-method
    // equality. The S1 branch lives in the canonical operators.js pyEq; the old
    // hand-inlined emit.rs copy lacked it, so `pyths run` diverged from the
    // compiled/package path (a.m == a.m printed False inline, True compiled).
    // pyEq is now #170-extracted — this corpus entry runs the INLINE codegen
    // under node, so the two-copy divergence cannot silently return.
    ("bound_method_eq", "class A:\n    def __init__(self): self.i = 5\n    def m(self): return self.i\na = A()\nb = A()\nprint(a.m == a.m, a.m in [a.m], b.m == a.m, a.m != a.m)", "True True False False\n"),
    // 0.2.2 conformance review FIX 2: `@=` dispatches __imatmul__ when defined
    // (name AND subscript aug-assign paths), falling back to __matmul__ when not.
    ("imatmul_dispatch", "class M:\n    def __init__(self, v): self.v = v\n    def __matmul__(self, o): return M(self.v + '@' + o.v)\n    def __imatmul__(self, o):\n        self.v += 'i@' + o.v\n        return self\nclass N:\n    def __init__(self, v): self.v = v\n    def __matmul__(self, o): return N(self.v + '@' + o.v)\nm = M('a')\nm @= M('b')\nn = N('x')\nn @= N('y')\nxs = [M('z')]\nxs[0] @= M('w')\nprint(m.v, n.v, xs[0].v)", "ai@b x@y zi@w\n"),
    // 0.2.2 conformance review FIX 3: user-class __mro__ ends in a base whose
    // __name__ is 'object' (not the internal 'PyObject'), and the interned
    // builtin type objects carry a real __mro__ (int → [int, object],
    // bool → [bool, int, object]) so `int.__mro__[-1]` is `object`, not a
    // None-subscript TypeError.
    ("mro_surface", "class B: pass\nclass D(B): pass\nprint([c.__name__ for c in D.__mro__])\nprint(int.__mro__[-1], [c.__name__ for c in bool.__mro__], str.__mro__[-1] == object)", "['D', 'B', 'object']\n<class 'object'> ['bool', 'int', 'object'] True\n"),
    // #453 (reserved internal codegen names): user variables named exactly like
    // the comprehension/genexp internals (`__result`, `__comp_it`, `__gen_it`)
    // must keep resolving to the USER binding inside the loop-path IIFE —
    // fresh_temp mints a collision-proof temp instead. CPython goldens.
    ("user_result_var_comp", "__result = 100\nvals = [__result + x for x in [1, 2] for y in [3]]\nprint(vals)\nprint(__result)", "[101, 102]\n100\n"),
    ("user_comp_it_var", "__comp_it = 5\nvals = [x + __comp_it for x in [1, 2] for y in [3]]\nprint(vals)", "[6, 7]\n"),
    ("user_gen_it_var", "__gen_it = 10\ng = (x + __gen_it for x in [1, 2])\nprint(list(g))", "[11, 12]\n"),
    ("user_result_dictcomp", "__result = 7\nd = {k: __result for k in [1, 2] for j in [3]}\nprint(d)", "{1: 7, 2: 7}\n"),
    // Nested loop-path comprehensions both mint the temp; the inner IIFE
    // shadows the outer exactly like Python's nested comprehension scopes.
    ("user_result_nested_comp", "__result = 1\nm = [[__result + i for i in [x] for _ in [0]] for x in [1, 2] for _ in [0]]\nprint(m)", "[[2], [3]]\n"),
    // #452 (builtin-named loop targets): a for / comprehension / genexp target
    // that shadows a builtin binds the LOOP VARIABLE (not the builtin value,
    // not a ReferenceError), while the iterable — evaluated BEFORE the target
    // binds — still resolves the builtin. CPython goldens. Each row verified
    // NON-VACUOUS: it fails on main without the fix (in-body-only reads avoid
    // the hoist that masked the target bug; the `list(...)` iterables hit the
    // enclosing-scope-evaluation bugs on every emission path).
    ("builtin_named_for_target", "total = 0\nfor list in [[1], [2, 3]]:\n    total += len(list)\nprint(total)", "3\n"),
    ("builtin_iter_builtin_target", "for list in list([[1], [2]]):\n    pass\nprint(list)", "[2]\n"),
    ("builtin_named_comp_target", "lists = [[1, 2], [3]]\nprint([list for list in lists])\nprint([x for list in lists for x in list])\nprint([x for list in list([[1], [2, 3]]) for x in list])", "[[1, 2], [3]]\n[1, 2, 3]\n[1, 2, 3]\n"),
    ("builtin_named_genexp_target", "g = (len(list) for list in list([[1], [2, 3]]))\nprint(sum(g))", "3\n"),
    ("builtin_named_dictcomp_target", "print({str(list[0]): len(list) for list in list([[1], [2, 3]])})", "{'1': 1, '2': 2}\n"),
    // #452 review blocker 1: the RECEIVER of an attribute STORE is a READ
    // context — a builtin-named VALUE inside it must still get the builtin
    // value mapping (the first in_lhs_target guard leaked the LHS flag into
    // the receiver: bare `list` → ReferenceError).
    ("attr_store_builtin_receiver", "class Box:\n    pass\nboxes = []\ndef wrap(t):\n    b = Box()\n    b.t = t\n    boxes.append(b)\n    return b\nwrap(list).t2 = 5\nprint(boxes[0].t is list, boxes[0].t2)", "True 5\n"),
    // #452 review blocker 2: sentinel reads are SCOPE-CHAIN aware.
    // (a) a nested function reading an unbound builtin-named MODULE global
    // falls through to the builtin (CPython's dynamic globals → builtins
    // chain), and sees the update once the loop binds it.
    ("global_sentinel_builtin_from_fn", "for list in []:\n    pass\ndef f():\n    return list\nprint(f() is list)\nfor list in [[7]]:\n    pass\nprint(f())", "True\n[7]\n"),
    // (a') a non-builtin unbound global read from a function raises NameError
    // (not UnboundLocalError, never the raw sentinel). The trailing
    // module-scope read forces the sentinel hoist — on main that made the
    // nested-fn read return the RAW `__UNBOUND` symbol instead of raising
    // (without it, main leaked a bare-const ReferenceError that the except
    // mapping masked as NameError — a vacuous pass).
    ("global_sentinel_nameerror_from_fn", "for q in []:\n    pass\ndef f():\n    try:\n        return q\n    except UnboundLocalError:\n        return 'ULE'\n    except NameError:\n        return 'NE'\nprint(f())\ntry:\n    print(q)\nexcept NameError:\n    print('outer NE')", "NE\nouter NE\n"),
    // (b) a closure over an unbound OUTER-function loop local raises the
    // free-variable NameError (not UnboundLocalError, never the sentinel) —
    // while the OWN-scope read stays UnboundLocalError. The own-scope read
    // comes FIRST so the outer function hoists the sentinel — on main the
    // closure read then returned the raw `__UNBOUND` symbol.
    ("free_var_sentinel_closure", "def outer():\n    for q in []:\n        pass\n    try:\n        r1 = q\n    except UnboundLocalError:\n        r1 = 'ULE'\n    def inner():\n        return q\n    try:\n        r2 = inner()\n    except UnboundLocalError:\n        r2 = 'iULE'\n    except NameError:\n        r2 = 'NE'\n    return (r1, r2)\nprint(outer())", "('ULE', 'NE')\n"),
    // (c) `global` / `nonlocal` augmented assignment on a sentinel: unbound
    // raises through the guarded READ side (NameError in both cases —
    // CPython 3.12), bound updates write back through the declaration. The
    // outer own-scope read in the nonlocal row forces the sentinel hoist
    // (main then fed the raw symbol into pyAdd → uncaught TypeError).
    ("global_sentinel_augassign", "for count in []:\n    pass\ndef f():\n    global count\n    try:\n        count += 1\n        return count\n    except UnboundLocalError:\n        return 'ULE'\n    except NameError:\n        return 'NE'\nprint(f())\nfor count in [10]:\n    pass\nprint(f(), count)", "NE\n11 11\n"),
    ("nonlocal_sentinel_augassign", "def outer():\n    for q in []:\n        pass\n    try:\n        q\n    except UnboundLocalError:\n        pass\n    def inner():\n        nonlocal q\n        try:\n            q += 1\n            return q\n        except UnboundLocalError:\n            return 'ULE'\n        except NameError:\n            return 'NE'\n    return inner()\nprint(outer())\ndef outer2():\n    for q in [5]:\n        pass\n    def inner():\n        nonlocal q\n        q += 1\n    inner()\n    return q\nprint(outer2())", "NE\n6\n"),
    // #454 (comprehension unification): ASYNC DICT comprehensions — the
    // per-form dict emitter had no async arm at all (neither the fast path
    // nor the loop path), so `{k: v async for ...}` compiled to a sync
    // `.map()` over a non-iterable async source and threw. The unified
    // lowering gives every form the same async arm. CPython golden. The
    // condition forces the (previously async-less) loop machinery.
    ("async_dictcomp", "import asyncio\nclass A:\n    def __init__(self, xs):\n        self.xs = xs\n        self.i = 0\n    def __aiter__(self):\n        return self\n    async def __anext__(self):\n        if self.i >= len(self.xs):\n            raise StopAsyncIteration\n        v = self.xs[self.i]\n        self.i += 1\n        return v\nasync def main():\n    r = {x: x * x async for x in A([1, 2, 3, 4]) if x % 2 == 0}\n    print(r)\nasyncio.run(main())", "{2: 4, 4: 16}\n"),
    // #454 sibling: MIXED async-outer + sync-inner generator levels flow
    // through the same unified loop emitter (per-level for-await decision).
    ("comp_mixed_async_inner", "import asyncio\nclass A:\n    def __init__(self, xs):\n        self.xs = xs\n        self.i = 0\n    def __aiter__(self):\n        print(\"aiter\")\n        return self\n    async def __anext__(self):\n        if self.i >= len(self.xs):\n            raise StopAsyncIteration\n        v = self.xs[self.i]\n        self.i += 1\n        return v\nasync def main():\n    r = [x * 10 + y async for x in A([1, 2]) for y in [1, 2]]\n    print(r)\nasyncio.run(main())", "aiter\n[11, 12, 21, 22]\n"),
    // #463: CPython calls iter(outermost) when the GENEXP OBJECT IS CREATED
    // (GET_ITER runs before the genexp function is called) — observable with
    // a THROWING __iter__: the ValueError must surface at creation, inside
    // the try, not at (deferred) consumption outside it.
    ("genexp_eager_iter_throw", "class T:\n    def __init__(self, xs):\n        self.xs = xs\n    def __iter__(self):\n        raise ValueError(\"boom\")\ntry:\n    g = (x for x in T([1]))\n    print(\"no raise\")\nexcept ValueError:\n    print(\"raised at creation\")", "raised at creation\n"),
    // #463 sibling: the eagerly-acquired iterator is acquired ONCE and is
    // the SAME iterator consumption continues from — `next(g)` then
    // `list(g)` resume one iterator; __iter__ runs exactly once, at creation.
    ("genexp_iter_once", "class T:\n    def __init__(self, xs):\n        self.xs = xs\n    def __iter__(self):\n        print(\"iter\")\n        return iter(self.xs)\ng = (x for x in T([1, 2, 3]))\nprint(next(g))\nprint(list(g))", "iter\n1\n[2, 3]\n"),
    // Bytes-completeness root fix (#455/#456/#457/#458): the bytes dispatch
    // authority guard. One row-set exercises the bytes/bytearray value across
    // the WHOLE surface -- truthiness, type()/__name__/isinstance identity,
    // slice read (kind-preserving), slice assign (grow/shrink/insert/self/
    // extended) + slice delete, element-write validation, direct method calls,
    // bound-method extraction, and the error KINDS+messages for the immutable/
    // invalid paths -- each golden from live CPython 3.12. A future bytes op
    // that bypasses the authority (__pyBytesKind / the PyBytes prototype
    // method surface) diverges here.
    ("bytes_truthiness", "print(bool(b\"\"), bool(b\"x\"), bool(bytearray()), bool(bytearray(b\"y\")))\nprint(\"T\" if b\"x\" else \"F\", \"T\" if b\"\" else \"F\", \"T\" if bytearray() else \"F\")", "False True False True\nT F F\n"),
    ("bytes_type_surface", "b = b\"ab\"\nba = bytearray(b\"ab\")\nprint(type(b).__name__, type(ba).__name__)\nprint(type(b) == bytes, type(ba) == bytearray, type(b) == bytearray)\nprint(isinstance(b, bytes), isinstance(ba, bytearray), isinstance(ba, bytes), isinstance(b, (int, bytes)))\nprint(bytes, bytearray)", "bytes bytearray\nTrue True False\nTrue True False True\n<class 'bytes'> <class 'bytearray'>\n"),
    ("bytes_slice_read_kind", "b = b\"banana\"\nba = bytearray(b\"banana\")\nprint(b[1:4], b[::-1], b[::2], ba[2:], ba[-3:-1])", "b'ana' b'ananab' b'bnn' bytearray(b'nana') bytearray(b'an')\n"),
    ("bytearray_slice_assign", "x = bytearray(b\"hello\")\nx[1:3] = b\"XY\"\nprint(x)\nx[1:3] = b\"LONGER\"\nprint(x, len(x))\nx[2:4] = []\nprint(x)\ny = bytearray(b\"abc\")\ny[2:0] = b\"QQ\"\nprint(y)\nz = bytearray(b\"abc\")\nz[1:3] = z\nprint(z)\nw = bytearray(b\"abc\")\nw[0:2] = (66, True)\nprint(w)", "bytearray(b'hXYlo')\nbytearray(b'hLONGERlo') 9\nbytearray(b'hLGERlo')\nbytearray(b'abQQc')\nbytearray(b'aabc')\nbytearray(b'B\\x01c')\n"),
    ("bytearray_slice_extended_del", "x = bytearray(b\"abcdef\")\nx[::2] = b\"XYZ\"\nprint(x)\ny = bytearray(b\"abcdef\")\ny[::-2] = b\"XYZ\"\nprint(y)\nd = bytearray(b\"abcdef\")\ndel d[1:3]\nprint(d)\ne = bytearray(b\"abcdef\")\ndel e[::2]\nprint(e)\nf = bytearray(b\"abcdef\")\ndel f[10:20]\nprint(f)\ng = bytearray(b\"abc\")\ng[-1] = 90\nprint(g)", "bytearray(b'XbYdZf')\nbytearray(b'aZcYeX')\nbytearray(b'adef')\nbytearray(b'bdf')\nbytearray(b'abcdef')\nbytearray(b'abZ')\n"),
    ("bytes_methods_direct", "b = b\"banana\"\nprint(b.count(b\"an\"), b.count(97), b.count(b\"\"), b.count(b\"\", 2, 4), b.count(97, 2, 4))\nprint(b.find(b\"na\"), b.find(b\"na\", -3), b.find(98), b.find(b\"zz\"), b.rfind(b\"na\"), b.rfind(b\"na\", 0, 4))\nprint(b.index(b\"an\", 1, 4), b.rindex(b\"na\"))\nprint(b.startswith(b\"ba\"), b.startswith((b\"x\", b\"ban\")), b.startswith(b\"na\", 2), b.endswith(b\"na\"), b.endswith(b\"an\", 0, 3))", "2 3 7 3 1\n2 4 0 -1 4 2\n1 4\nTrue True True True True\n"),
    ("bytes_methods_extracted", "b = b\"banana\"\nm = b.count\nprint(m(b\"an\"), m(97, 2))\nf = b.find\nprint(f(b\"na\"))\ns = bytearray(b\"banana\").startswith\nprint(s(b\"ban\"))\ni = b.index\nprint(i(b\"an\"))\nbc = bytearray(b\"banana\").count\nprint(bc(98))", "2 2\n2\nTrue\n1\n1\n"),
    ("bytes_write_error_kinds", "def t(f):\n    try:\n        f()\n        print(\"NOERR\")\n    except Exception as e:\n        print(type(e).__name__ + \":\", e)\ndef w1():\n    bb = b\"abc\"\n    bb[0] = 65\nt(w1)\ndef w2():\n    bb = b\"abc\"\n    bb[1:2] = b\"X\"\nt(w2)\ndef w3():\n    z = bytearray(b\"abcdef\")\n    z[::2] = b\"XY\"\nt(w3)\ndef w4():\n    z = bytearray(b\"abc\")\n    z[0:2] = \"xy\"\nt(w4)\ndef w5():\n    z = bytearray(b\"abc\")\n    z[0:2] = [300]\nt(w5)\ndef w6():\n    z = bytearray(b\"abc\")\n    z[10] = 1\nt(w6)\ndef w7():\n    z = bytearray(b\"abc\")\n    z[0] = 300\nt(w7)", "TypeError: 'bytes' object does not support item assignment\nTypeError: 'bytes' object does not support item assignment\nValueError: attempt to assign bytes of size 2 to extended slice of size 3\nTypeError: can assign only bytes, buffers, or iterables of ints in range(0, 256)\nValueError: byte must be in range(0, 256)\nIndexError: bytearray index out of range\nValueError: byte must be in range(0, 256)\n"),
    ("bytes_method_error_kinds", "b = b\"banana\"\ndef t(f):\n    try:\n        f()\n        print(\"NOERR\")\n    except Exception as e:\n        print(type(e).__name__ + \":\", e)\nt(lambda: b.count(\"x\"))\nt(lambda: b.count(300))\nt(lambda: b.index(b\"zz\"))\nt(lambda: b.startswith(\"ba\"))\nt(lambda: b.startswith(98))\nt(lambda: b.find(1.5))", "TypeError: argument should be integer or bytes-like object, not 'str'\nValueError: byte must be in range(0, 256)\nValueError: subsection not found\nTypeError: startswith first arg must be bytes or a tuple of bytes, not str\nTypeError: startswith first arg must be bytes or a tuple of bytes, not int\nTypeError: argument should be integer or bytes-like object, not 'float'\n"),
];

/// Compile each (name, src, expected) row inline, run under node, and return
/// the failure descriptions (empty = all rows byte-identical to CPython).
fn run_rows(tag: &str, rows: &[(String, String, String)]) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("pyths_behdiff_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    pyths_runtime::materialize_runtime_package(&dir).expect("materialize runtime");

    let mut failures = Vec::new();
    for (name, src, expected) in rows {
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
    failures
}

#[test]
fn behavioral_differential_matches_cpython() {
    let rows: Vec<(String, String, String)> = CORPUS
        .iter()
        .map(|(n, s, e)| (n.to_string(), s.to_string(), e.to_string()))
        .collect();
    let failures = run_rows("corpus", &rows);
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

/// Recurrence guard for the comprehension-lowering CLASS bug (#454, #463).
///
/// History: comprehension/genexp lowering was four per-form emitters, and
/// features (async arms, eager iter timing) got bolted onto SOME forms but
/// not others — ~5 recurrences, most recently "the dict path has no async
/// arm" (#454) and "genexps defer iter(outermost) to consumption" (#463).
///
/// This test enumerates the FULL matrix {list, set, dict, gen} x {sync,
/// async} in code — the rows are BUILT by iterating every `Form` for both
/// async-nesses, so a per-form omission cannot slip through as a missing
/// hand-written row. Each cell uses the same probe skeleton:
///   - sync: a side-effecting `__iter__` (prints "iter") observes that the
///     source is iterated through the Python protocol exactly once, and —
///     for the genexp cell — that iter(outermost) is acquired at CREATION
///     time ("iter" prints BEFORE "made"), matching CPython's GET_ITER
///     placement (dis: GET_ITER runs before the genexp function is called).
///   - async: a side-effecting `__aiter__` + genuinely-awaited `__anext__`
///     (a protocol class, NOT a native async generator — so a sync-lowered
///     arm cannot accidentally pass), with the same creation-time probe for
///     the genexp cell (GET_AITER).
/// Every expected string is a live-CPython golden (verified by running the
/// exact same source under python; see the gen script in the PR).
#[test]
fn comprehension_matrix_matches_cpython() {
    const SYNC_PRELUDE: &str = "class T:\n    def __init__(self, xs):\n        self.xs = xs\n    def __iter__(self):\n        print(\"iter\")\n        return iter(self.xs)\n";
    const ASYNC_PRELUDE: &str = "import asyncio\nclass A:\n    def __init__(self, xs):\n        self.xs = xs\n        self.i = 0\n    def __aiter__(self):\n        print(\"aiter\")\n        return self\n    async def __anext__(self):\n        if self.i >= len(self.xs):\n            raise StopAsyncIteration\n        v = self.xs[self.i]\n        self.i += 1\n        return v\n";

    #[derive(Clone, Copy)]
    enum Form {
        List,
        Set,
        Dict,
        Gen,
    }
    const ALL_FORMS: [Form; 4] = [Form::List, Form::Set, Form::Dict, Form::Gen];

    let mut rows: Vec<(String, String, String)> = Vec::new();
    for form in ALL_FORMS {
        for is_async in [false, true] {
            let afor = if is_async { "async for" } else { "for" };
            let cls = if is_async { "A" } else { "T" };
            // (body lines after the comprehension, expected value line)
            let (name, comp, consume, value) = match form {
                Form::List => (
                    "list",
                    format!("r = [x * 2 {afor} x in {cls}([1, 2])]"),
                    "print(r)".to_string(),
                    "[2, 4]",
                ),
                Form::Set => (
                    "set",
                    format!("r = {{x * 2 {afor} x in {cls}([1, 2])}}"),
                    "print(sorted(r))".to_string(),
                    "[2, 4]",
                ),
                Form::Dict => (
                    "dict",
                    format!("r = {{x: x * 2 {afor} x in {cls}([1, 2])}}"),
                    "print(r)".to_string(),
                    "{1: 2, 2: 4}",
                ),
                Form::Gen => (
                    "gen",
                    format!("g = (x * 2 {afor} x in {cls}([1, 2]))"),
                    if is_async {
                        "r = [v async for v in g]\nprint(r)".to_string()
                    } else {
                        "print(list(g))".to_string()
                    },
                    "[2, 4]",
                ),
            };
            let body = format!("{comp}\nprint(\"made\")\n{consume}");
            let (src, probe) = if is_async {
                // Async cells must sit inside an async def in CPython too —
                // asyncio.run keeps the source runnable under real python.
                let indented = body.replace('\n', "\n    ");
                (
                    format!(
                        "{ASYNC_PRELUDE}async def main():\n    {indented}\nasyncio.run(main())"
                    ),
                    "aiter",
                )
            } else {
                (format!("{SYNC_PRELUDE}{body}"), "iter")
            };
            // CPython golden ordering: EVERY form acquires the (a)iterator
            // before "made" is printed — eagerly for list/set/dict (the
            // whole comprehension runs first), and AT CREATION for genexps
            // (GET_ITER/GET_AITER precede the genexp call).
            let expected = format!("{probe}\nmade\n{value}\n");
            let mode = if is_async { "async" } else { "sync" };
            rows.push((format!("matrix_{name}_{mode}"), src, expected));
        }
    }
    assert_eq!(rows.len(), 8, "full form x async matrix");

    let failures = run_rows("matrix", &rows);
    assert!(
        failures.is_empty(),
        "{} comprehension-matrix difference(s) vs CPython:
{}",
        failures.len(),
        failures.join(
            "
"
        )
    );
}

/// #441 + the EVAL-TIMING class: CPython evaluates a `def` statement's
/// DEFAULTS (left-to-right) and then its ANNOTATIONS (params
/// left-to-right, then the return annotation) when the def EXECUTES — in
/// the enclosing scope — so a walrus in any of them assigns its
/// enclosing-scope target at def time. The matrix covers {annotation,
/// default, return-annotation} x {module-level, nested def}, plus the
/// defaults-before-annotations ordering probe. Same eval-timing fidelity
/// family as the genexp creation-time iter() rows above (#463). All
/// goldens from live CPython 3.12.
#[test]
fn def_time_eval_matches_cpython() {
    let rows: Vec<(String, String, String)> = [
        // Annotation walrus, nested def (#441's literal repro): `n` must be
        // assigned in outer's scope when `def inner` executes.
        (
            "ann_walrus_nested",
            "def outer():\n    def inner(x: (n := 5)):\n        return x\n    return n\nprint(outer())",
            "5\n",
        ),
        // Default walrus, nested def: def-time (F6 hoist) AND the target
        // hoisted in the enclosing scope (was a strict-mode ReferenceError:
        // the hoist scan never reached nested-def params).
        (
            "default_walrus_nested",
            "def outer():\n    def inner(x=(n := 7)):\n        return x\n    return (n, inner())\nprint(outer())",
            "(7, 7)\n",
        ),
        // Module-level annotation walrus.
        (
            "ann_walrus_module",
            "def f(x: (m := 3)): pass\nprint(m)",
            "3\n",
        ),
        // Return-annotation walrus (evaluated last, still at def time).
        (
            "ret_ann_walrus",
            "def g() -> (r := 9): pass\nprint(r)",
            "9\n",
        ),
        // Multiple params: every default and every annotation fires.
        (
            "deftime_all_fire",
            "def h(x: (a := 1) = (b := 2), y: (c := 3) = (d := 4)): pass\nprint(a, b, c, d)",
            "1 2 3 4\n",
        ),
        // ORDER probe: defaults evaluate BEFORE annotations (dis: defaults
        // tuple is built, then the annotations) — the annotation reads the
        // value the default's walrus just assigned.
        (
            "deftime_defaults_before_annotations",
            "def h(x: (order := order + 'a') = (order := 'd')): pass\nprint(order)",
            "da\n",
        ),
    ]
    .iter()
    .map(|(n, s, e)| (n.to_string(), s.to_string(), e.to_string()))
    .collect();

    let failures = run_rows("deftime", &rows);
    assert!(
        failures.is_empty(),
        "{} def-time eval difference(s) vs CPython:
{}",
        failures.len(),
        failures.join(
            "
"
        )
    );
}
