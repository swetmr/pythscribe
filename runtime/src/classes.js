// PythScribe cooperative-MRO object model.
//
// Mirrors the inline copy emitted by `pyths run`
// (crates/pyths_codegen_js/src/emit.rs :: PY_OBJECT_MODEL_JS). Keep the two
// in sync.
//
//  * `PyObject`     — root base for every regular class. Its constructor
//                     dispatches to the first `__init__` in `new.target`'s
//                     MRO; cooperative `super().__init__()` chains the rest.
//  * `__pyC3`       — C3 linearization (the same merge CPython uses) → `__mro__`.
//  * `__pyClass`    — installs `__mro__`/`__bases__` (PyObject appended as the
//                     root) and mixes in methods from non-first bases (first
//                     MRO definer wins → Python override order).
//  * `__pySuper`    — cooperative super proxy: dispatches to the next class
//                     *after the defining class* in the instance's MRO, so
//                     diamonds chain L→R→Base.
//  * `__pyIsInstance` — isinstance() that consults the MRO (so non-first
//                     bases match), with tuple-of-types support.

// Bytes dispatch authority (runtime.js) — the ONE bytes/bytearray
// recognizer, used by the isinstance sentinel cases below. Import sits
// ABOVE the first top-level declaration on purpose: the #170 slicer
// excludes pre-declaration lines, and the inline `pyths run` mirror pulls
// the same __pyBytesKind definition via the extractor instead.
import { __pyBytesKind } from "./runtime.js";

// Diamond-inheritance marker. `__pyClass` flattens each class's MRO methods
// onto its own prototype, so a base's prototype carries COPIES of ancestor
// methods it did not define in its body. This Symbol tags those copies so a
// later mixin (and cooperative `super()`) can tell a genuine body-definition
// apart from an inherited flattened copy — the fix for `class D(B, C)` where B
// carries a flattened copy of A's method that must NOT mask C's real override.
const __PY_MIXIN = Symbol("pyMixin");

export class PyObject {
    // PyObject IS CPython's `object` — the implicit root every user-class
    // MRO ends in. Stamp the Python name so surfaces that read __name__
    // (`[c.__name__ for c in D.__mro__]`, repr) report 'object', not the
    // internal JS class name 'PyObject' (the `__name__ ?? name` fallback
    // in the codegen's attribute lowering would otherwise leak it).
    static __name__ = "object";
    constructor(...args) {
        const cls = new.target;
        const mro = cls && cls.__mro__ ? cls.__mro__ : [cls];
        for (const c of mro) {
            if (c && c.prototype && Object.prototype.hasOwnProperty.call(c.prototype, "__init__")) {
                c.prototype.__init__.apply(this, args);
                return;
            }
        }
    }
    __init__() {}
}
PyObject.__mro__ = [PyObject];

export function __pyC3(cls, bases) {
    const seqs = bases.map((b) => (b && b.__mro__ ? b.__mro__.slice() : [b]));
    if (bases.length) seqs.push(bases.slice());
    const result = [cls];
    while (true) {
        const nonEmpty = seqs.filter((s) => s.length > 0);
        if (nonEmpty.length === 0) break;
        let cand = null;
        for (const s of nonEmpty) {
            const head = s[0];
            const inTail = nonEmpty.some((o) => o.indexOf(head) > 0);
            if (!inTail) { cand = head; break; }
        }
        if (cand === null) {
            const e = new Error("Cannot create a consistent method resolution order (MRO)");
            e.name = "TypeError"; throw e;
        }
        result.push(cand);
        for (const s of nonEmpty) { if (s[0] === cand) s.shift(); }
    }
    return result;
}

export function __pyClass(cls, bases) {
    cls.__bases__ = bases;
    const mro = __pyC3(cls, bases);
    if (mro.indexOf(PyObject) < 0) mro.push(PyObject);
    cls.__mro__ = mro;
    const proto = cls.prototype;
    // Diamond-safe method flattening. A name resolves to the FIRST class in the
    // MRO whose *body* defines it (CPython semantics). A base's prototype may
    // carry flattened COPIES of ancestor methods from its own `__pyClass` pass;
    // those are recorded in the base's `__PY_MIXIN` set and must NOT count as
    // the base "defining" the method — otherwise B's flattened copy of A.who
    // would be installed ahead of C's genuine override in `class D(B, C)`.
    const bodyNames = new Set(Object.getOwnPropertyNames(proto)); // defined in cls's body
    bodyNames.delete("constructor");
    const finalized = new Set(bodyNames); // names already bound to their winner
    const copied = new Set();             // names this class flattened in (not body-defined)
    for (let i = 1; i < mro.length; i++) {
        const base = mro[i];
        if (!base || !base.prototype || base === PyObject) continue;
        const baseMixed = Object.prototype.hasOwnProperty.call(base.prototype, __PY_MIXIN)
            ? base.prototype[__PY_MIXIN]
            : null;
        for (const name of Object.getOwnPropertyNames(base.prototype)) {
            if (name === "constructor") continue;
            if (finalized.has(name)) continue;              // an earlier genuine definer already won
            if (baseMixed && baseMixed.has(name)) continue; // base only inherited it → keep scanning the MRO
            Object.defineProperty(proto, name, Object.getOwnPropertyDescriptor(base.prototype, name));
            finalized.add(name);
            copied.add(name);
        }
    }
    Object.defineProperty(proto, __PY_MIXIN, { value: copied, enumerable: false, writable: true, configurable: true });
    // Round-3 pythonic sweep: iteration-protocol bridges. A class
    // defining __iter__ becomes JS-iterable (for-of, list(), spread,
    // Array.from); a class defining __next__ gets a JS-iterator `next`
    // adapter (StopIteration → done), so `__iter__(self): return self`
    // iterators work end-to-end.
    if (typeof proto.__iter__ === "function" && !proto[Symbol.iterator]) {
        Object.defineProperty(proto, Symbol.iterator, {
            value() {
                const it = this.__iter__();
                if (it != null && typeof it.next === "function") return it;
                return it[Symbol.iterator]();
            },
            writable: true,
            configurable: true,
        });
    }
    if (typeof proto.__next__ === "function" && typeof proto.next !== "function") {
        Object.defineProperty(proto, "next", {
            value() {
                try {
                    return { value: this.__next__(), done: false };
                } catch (e) {
                    if (e && e.name === "StopIteration") return { value: undefined, done: true };
                    throw e;
                }
            },
            writable: true,
            configurable: true,
        });
    }
    return cls;
}

export function __pySuper(startCls, inst) {
    const ctor = inst.constructor;
    const mro = ctor && ctor.__mro__ ? ctor.__mro__ : [ctor];
    const idx = mro.indexOf(startCls);
    const after = idx >= 0 ? mro.slice(idx + 1) : [];
    return new Proxy(Object.create(null), {
        get(_t, prop) {
            for (const c of after) {
                if (!c || !c.prototype) continue;
                if (!Object.prototype.hasOwnProperty.call(c.prototype, prop)) continue;
                // Skip flattened (mixed-in) copies — cooperative super() must
                // dispatch to the next class that GENUINELY defines `prop` in
                // its body, so a diamond's shared branch is not skipped.
                const mixed = Object.prototype.hasOwnProperty.call(c.prototype, __PY_MIXIN)
                    ? c.prototype[__PY_MIXIN]
                    : null;
                if (mixed && typeof prop === "string" && mixed.has(prop)) continue;
                const val = c.prototype[prop];
                return typeof val === "function" ? val.bind(inst) : val;
            }
            return undefined;
        },
    });
}

export function __pyIsInstance(obj, cls) {
    // Builtin TYPE names arrive as string sentinels (the codegen lowers
    // isinstance(x, list) to __pyIsInstance(x, "list") — `list` has no JS
    // value). Documented residual: whole floats report as int, and unmarked
    // (derived) tuples report as list — both fundamental to representing
    // Python ints/floats/tuples as bare JS numbers/arrays at runtime.
    // (#215: `isinstance(True, int)` now correctly True — bool ⊆ int.)
    if (typeof cls === "string") {
        switch (cls) {
            case "list": return Array.isArray(obj) && !obj.__pytuple__;
            case "tuple": return Array.isArray(obj) && !!obj.__pytuple__;
            case "str": return typeof obj === "string";
            case "bool": return typeof obj === "boolean";
            // bool is a subclass of int in Python, so a boolean is an int.
            case "int": return typeof obj === "boolean" || typeof obj === "bigint" || (typeof obj === "number" && Number.isInteger(obj));
            case "float": return (typeof obj === "number" && !Number.isInteger(obj)) || (obj != null && obj.__pyfloat__ === true);
            case "dict": return obj !== null && typeof obj === "object" && (Object.getPrototypeOf(obj) === Object.prototype || obj instanceof Map);
            case "set": case "frozenset": return obj instanceof Set;
            // Bytes authority: bytearray is NOT a bytes subclass in CPython,
            // so each name matches exactly its own kind.
            case "bytes": return __pyBytesKind(obj) === "bytes";
            case "bytearray": return __pyBytesKind(obj) === "bytearray";
            case "NoneType": return obj === null || obj === undefined;
            // Everything is an instance of object in Python.
            case "object": return true;
        }
        return false;
    }
    // Interned CALLABLE type objects (int/list/dict/object/… as VALUES —
    // runtime.js __pyType* singletons) dispatch through the string path.
    if (typeof cls === "function" && cls.__pytype__ === true) {
        return __pyIsInstance(obj, cls.__name__);
    }
    // `type(x)` for a builtin returns an interned type OBJECT (__PyTypeObj)
    // whose __name__ is the CPython type name, not a JS constructor. Route
    // `isinstance(x, type(x))` through the string path via that name.
    if (cls !== null && typeof cls === "object" && typeof cls.__name__ === "string" && cls.constructor && cls.constructor.name === "__PyTypeObj") {
        return __pyIsInstance(obj, cls.__name__);
    }
    if (typeof cls !== "function" && cls != null && typeof cls[Symbol.iterator] === "function") {
        for (const c of cls) { if (__pyIsInstance(obj, c)) return true; }
        return false;
    }
    if (obj == null) return false;
    try { if (obj instanceof cls) return true; } catch (_e) {}
    const ctor = obj.constructor;
    const mro = ctor && ctor.__mro__ ? ctor.__mro__ : null;
    return mro ? mro.indexOf(cls) >= 0 : false;
}

// autotester arguments: keyword form of __pyClassCall. Statics and
// classmethods bind keywords against their own metadata; a plain instance
// method called UNBOUND (`A.__init__(self, y, x, m=n, ...)`) carries self
// as the FIRST positional, which its __pyparams__ metadata does not list —
// bind keywords against the remaining positionals and re-prepend self.
export function __pyClassCallKw(cls, name, pos, kw) {
    const s = cls[name];
    if (typeof s === "function" && /^class[\s{(]/.test(Function.prototype.toString.call(s))) {
        return new s(...__pyKwArgs(s, pos, kw));
    }
    if (typeof s === "function") {
        return cls[name](...__pyKwArgs(s, pos, kw));
    }
    const m = cls.prototype ? cls.prototype[name] : undefined;
    if (typeof m === "function") {
        // __init__ metadata may live on the CLASS object (the emitter's
        // post-class assignments) rather than the prototype method.
        const meta = m.__pyparams__ ? m : (cls.__pyparams__ ? cls : m);
        return m.call(pos[0], ...__pyKwArgs(meta, pos.slice(1), kw));
    }
    const e = new Error(`type object '${cls.name}' has no attribute '${name}'`);
    e.name = "AttributeError";
    throw e;
}

// autotester classes: issubclass() over compiled classes (via __mro__ /
// the prototype chain), interned builtin type objects (runtime.js
// __pyType* singletons / string sentinels), tuples of classinfos, and the
// CPython special cases (everything subclasses object; bool ⊆ int).
export function __pyIsSubclass(cls, info) {
    // CPython type name of a builtin classinfo, else null.
    const tyName = (c) => {
        if (typeof c === "string") return c;
        if (typeof c === "function" && c.__pytype__ === true) return c.__name__;
        if (c !== null && typeof c === "object" && typeof c.__name__ === "string"
            && c.constructor && c.constructor.name === "__PyTypeObj") return c.__name__;
        return null;
    };
    const cn = tyName(cls);
    if (cn === null && typeof cls !== "function") {
        const e = new Error("issubclass() arg 1 must be a class");
        e.name = "TypeError"; throw e;
    }
    // Tuple form: any match wins.
    if (info !== null && typeof info === "object" && typeof info[Symbol.iterator] === "function") {
        for (const c of info) { if (__pyIsSubclass(cls, c)) return true; }
        return false;
    }
    const ci = tyName(info);
    if (ci === "object") return true; // everything is a subclass of object
    if (cn !== null) {
        // builtin type vs builtin type: identity + the bool ⊆ int special case.
        if (ci === null) return false;
        return cn === ci || (cn === "bool" && ci === "int");
    }
    // compiled/user class cls
    if (typeof info === "function" && info.__pytype__ !== true) {
        if (cls === info) return true;
        const mro = cls.__mro__;
        if (mro && mro.indexOf(info) >= 0) return true;
        let p = cls;
        while ((p = Object.getPrototypeOf(p))) { if (p === info) return true; }
        return false;
    }
    return false; // user class never subclasses a builtin type name
}

// Round-3 pythonic sweep: Python class attributes. The attribute lives on
// the class object (static chain gives subclass fallthrough for
// `Sub.attr`); a live prototype accessor makes instances read through and
// turns instance assignment into an own-property shadow — Python's
// attribute lookup semantics.
// autotester properties: the `property(fget, fset)` BUILTIN as a value —
// `x = property(getX, setX)` in a class body. Returns a marked descriptor
// record; __pyClassAttr installs it as a real accessor pair.
export function pyProperty(fget, fset, fdel, doc) {
    return { __pyproperty__: true, fget, fset, fdel, doc };
}

export function __pyClassAttr(cls, name, value) {
    cls[name] = value; // Cls.attr reads the raw value (property object incl.)
    if (value !== null && typeof value === "object" && value.__pyproperty__) {
        Object.defineProperty(cls.prototype, name, {
            get() { return value.fget ? value.fget.call(this) : undefined; },
            set(v) {
                if (!value.fset) {
                    const e = new Error("can't set attribute");
                    e.name = "AttributeError";
                    throw e;
                }
                value.fset.call(this, v);
            },
            configurable: true,
        });
        return;
    }
    Object.defineProperty(cls.prototype, name, {
        get() { return cls[name]; },
        set(v) { Object.defineProperty(this, name, { value: v, writable: true, enumerable: true, configurable: true }); },
        configurable: true,
    });
}

// Round-3 pythonic sweep: calling through a class object,
// `Cls.method(self, ...)` (unbound-method style) / `Cls.static(...)` /
// `Cls.classmethod(...)`. Statics and classmethods resolve on the class
// (static chain → `this` stays the class); plain methods resolve on the
// prototype with the first argument as `self`.
export function __pyClassCall(cls, name, args) {
    const s = cls[name];
    // autotester classes: a NESTED class installed as a class attribute
    // (`Outer.Inner(...)`) must construct with `new` — a plain call throws
    // "Class constructor cannot be invoked without 'new'".
    if (
        typeof s === "function" &&
        /^class[\s{(]/.test(Function.prototype.toString.call(s))
    ) {
        return new s(...args);
    }
    if (typeof s === "function") return cls[name](...args);
    const m = cls.prototype ? cls.prototype[name] : undefined;
    if (typeof m === "function") return m.call(args[0], ...args.slice(1));
    const e = new Error(`type object '${cls.name}' has no attribute '${name}'`);
    e.name = "AttributeError";
    throw e;
}

//# sourceMappingURL=classes.js.map
