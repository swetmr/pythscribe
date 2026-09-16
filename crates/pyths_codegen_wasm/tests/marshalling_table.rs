//! JS<->WASM marshalling-table binding tests (the Rust half of the two-sided
//! gate; the Lean half is `lake exe expanddiff --check-marshalling-table`).
//! See `bridge::marshalling_table()` and verification/marshalling-table.txt.

use pyths_codegen_wasm::bridge::marshalling_table;

#[test]
fn marshalling_table_matches_committed_fixture() {
    // The same fixture is independently regenerated and compared by the Lean
    // model (`expanddiff --check-marshalling-table`) — if either side changes
    // a conversion (`convert_js_to_wasm` / `convert_wasm_to_js` /
    // `list_elem_kind`), a boundary admission bit (`is_numeric_kernel_param` /
    // `is_scalar_wasm_return`), or a failure disposition (the emitted guard /
    // ladder snippets), its own gate fails until model, fixture, and
    // implementation agree again.
    let fixture = include_str!("../../../verification/marshalling-table.txt");
    let ours = marshalling_table();
    assert_eq!(
        fixture.replace("\r\n", "\n"),
        ours,
        "\nJS<->WASM marshalling rules changed — update \
         verification/marshalling-table.txt AND the Lean MarshalTable model \
         together (regenerate with `lake exe expanddiff --print-marshalling-table`)"
    );
}

#[test]
fn admitted_arg_rows_use_exact_marshallers() {
    // Finite witness of the Lean `marshal_param_admitted_sound`: every arg row
    // the #364 boundary ADMITS (bit=1) marshals through one of the value-exact
    // numeric converters — never the str/dict/tuple/closure marshallers and
    // never a `-` (no representation). The i64 converters' exactness is the
    // guarded discipline pinned by the `fault` rows + the Lean value lemmas.
    let exact: [&str; 16] = [
        "(typeof x === \"bigint\" ? x : BigInt(Math.trunc(x)))",
        // #38/#461/#465: f64 args are identity on a native Number, with the
        // standard int→float coercion on the hybrid BigInt form (ToNumber
        // at the WASM JS-API boundary throws on BigInt) and an OverflowError
        // past double range (__f64Arg = the glue-local mirror of __reqNum;
        // never a silent Infinity crossing).
        "__f64Arg(x)",
        "x ? 1 : 0",
        "__list_to_wasm(x, \"i64\")",
        "__list_to_wasm(x, \"f64\")",
        "__list_to_wasm(x, \"i32\")", // exact for ADMITTED lists: elements are bool (0/1)
        // M2a-4/M2b: the 10 admitted array crossings (5 dtypes × ndim {1, 2}).
        // `__array_to_wasm` runs the TOTAL runtime dtype/ndim/contiguity check
        // and bulk-copies at the exact dtype width (or reroutes to the JS twin /
        // throws) — the width-exact, refuse-on-mismatch class (see the Lean
        // `argValueExact (.ptrArray _ _)` and the per-dtype semantic-forcing
        // theorems + the 2-D row-major offset theorem), never a silent misread.
        "__array_to_wasm(x, \"int32\", 1)",
        "__array_to_wasm(x, \"int32\", 2)",
        "__array_to_wasm(x, \"int64\", 1)",
        "__array_to_wasm(x, \"int64\", 2)",
        "__array_to_wasm(x, \"float32\", 1)",
        "__array_to_wasm(x, \"float32\", 2)",
        "__array_to_wasm(x, \"float64\", 1)",
        "__array_to_wasm(x, \"float64\", 2)",
        "__array_to_wasm(x, \"uint8\", 1)",
        "__array_to_wasm(x, \"uint8\", 2)",
    ];
    let mut admitted = 0;
    for line in marshalling_table().lines() {
        let Some(rest) = line.strip_prefix("arg ") else {
            continue;
        };
        let (_, rhs) = rest.split_once(" -> ").expect("row shape");
        let (bit, expr) = rhs.split_once(" ; ").expect("row shape");
        if bit == "1" {
            admitted += 1;
            assert!(
                exact.contains(&expr),
                "ADMITTED arg row marshals through a non-exact converter: {line}"
            );
        }
    }
    // int, float, bool, list<int>, list<float>, list<bool> + the 10 admitted
    // arrays (5 dtypes × ndim {1, 2}) — the #364 surface, M2a-4/M2b.
    assert_eq!(admitted, 16, "admitted-param surface changed size");
}

#[test]
fn admitted_ret_rows_are_scalar_or_void() {
    // Finite witness of the Lean `marshal_ret_admitted_sound`: every ret row
    // #364 admits is a numeric scalar normalization or a void return — never
    // a pointer conversion (the class #364 exists to keep off the fast path).
    // Option B: `__f64Box(x)` joined the scalar class — a value-identical
    // tagged box for integer-valued f64 results (float identity), not a
    // pointer conversion.
    let exact: [&str; 5] = ["__i64ToJs(x)", "__f64Box(x)", "x", "Boolean(x)", "-"];
    let mut admitted = 0;
    for line in marshalling_table().lines() {
        let Some(rest) = line.strip_prefix("ret ") else {
            continue;
        };
        let (_, rhs) = rest.split_once(" -> ").expect("row shape");
        let (bit, expr) = rhs.split_once(" ; ").expect("row shape");
        if bit == "1" {
            admitted += 1;
            assert!(
                exact.contains(&expr),
                "ADMITTED ret row marshals through a non-scalar converter: {line}"
            );
        }
    }
    // int, float, bool, none, void.
    assert_eq!(admitted, 5, "admitted-return surface changed size");
}

#[test]
fn marshalling_table_checker_rejects_forged_row() {
    // The gate is byte equality, so a single drifted/forged disposition must
    // fail the comparison — e.g. an attacker/regression flipping the i64
    // element-overflow disposition back to the pre-guard silent-wrap shape.
    let fixture = include_str!("../../../verification/marshalling-table.txt").replace("\r\n", "\n");
    let forged = fixture.replace(
        "fault list-elem-i64-oob twins -> reroute-twin",
        "fault list-elem-i64-oob twins -> silent-wrap",
    );
    assert_ne!(forged, fixture, "forgery must actually change the row");
    assert_ne!(
        forged,
        marshalling_table(),
        "a forged disposition row must NOT match the derived table"
    );
}

#[test]
fn every_disposition_class_is_witnessed() {
    // Non-vacuity: the table is non-empty and every disposition class the
    // model distinguishes actually occurs in it (no dead class, no empty
    // section).
    let table = marshalling_table();
    for needle in [
        "arg ",
        "ret ",
        "fault ",
        "-> reroute-twin",
        "-> throw-range",
        "-> throw-overflow",
        "-> propagate-trap",
        "-> propagate-py",
        " ; -",
        "__list_to_wasm",
        "__list_write_back", // #484: symmetric write-back rows (Section 1b)
        "wb list<int> -> __list_write_back(x, p, \"i64\")",
        "__array_to_wasm",    // M2a-4: the 1-D array crossings
        "__array_write_back", // M2a-4: symmetric array write-back rows
        "wb array<int32,1> -> __array_write_back(x, p, \"int32\", 1)",
        "arg array<uint8,1> -> 1 ; __array_to_wasm(x, \"uint8\", 1)",
        "__dict_to_wasm",
        "__tuple_to_wasm",
        "__closure_to_wasm",
        "__i64ToJs",
    ] {
        assert!(
            table.contains(needle),
            "disposition class unwitnessed in table: {needle}"
        );
    }
    // 66 conversion rows (33 shapes × arg/ret) + 17 write-back rows (7 list #484
    // + 10 array: 5 dtypes × ndim {1, 2}, M2a-4/M2b) + 10 fault rows.
    assert_eq!(table.lines().count(), 93, "table row count drifted");
    // Every write-back row's shape has a matching arg row that marshals IN
    // through the SAME channel (`__list_to_wasm` for lists, `__array_to_wasm`
    // for arrays) — the symmetric-marshalling invariant, checkable from the
    // table itself.
    let wb_shapes: Vec<&str> = table
        .lines()
        .filter_map(|l| l.strip_prefix("wb "))
        .filter_map(|r| r.split_once(" -> ").map(|(s, _)| s))
        .collect();
    assert_eq!(wb_shapes.len(), 17, "write-back row count drifted");
    for shape in &wb_shapes {
        let channel = if shape.starts_with("array<") {
            "__array_to_wasm"
        } else {
            "__list_to_wasm"
        };
        assert!(
            table.contains(&format!("arg {} -> ", shape))
                && table
                    .lines()
                    .any(|l| l.starts_with(&format!("arg {} -> ", shape)) && l.contains(channel)),
            "write-back shape {shape} has no matching {channel} arg row (asymmetry)"
        );
    }
}
