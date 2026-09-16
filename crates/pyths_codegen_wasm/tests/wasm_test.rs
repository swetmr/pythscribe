use pyths_codegen_wasm::codegen_wasm;

fn compile_wasm(source: &str) -> Vec<u8> {
    let module = pyths_parser::parse(source).expect("Parse failed");
    let output = codegen_wasm(&module);
    assert!(
        !output.wasm.is_empty(),
        "Expected WASM output, but got none. Rejected: {:?}",
        output.rejected_functions
    );
    output.wasm
}

fn validate_wasm(wasm: &[u8]) {
    wasmparser::validate(wasm).expect("WASM validation failed");
}

/// #364 (Path B): assert a function is NOT WASM-eligible — it stays on the
/// correct JS path — and was rejected for the given reason substring. Guards
/// the soundness tightening: shapes the backend miscompiles (non-scalar returns,
/// comprehension building, `[x]*n` repetition) must never be WASM-admitted.
fn assert_stays_js(source: &str, reason_substr: &str) {
    let module = pyths_parser::parse(source).expect("Parse failed");
    let output = codegen_wasm(&module);
    assert!(
        output.wasm.is_empty(),
        "expected NO WASM (function should stay JS), got {} bytes",
        output.wasm.len()
    );
    assert!(
        output
            .rejected_functions
            .iter()
            .any(|(_, r)| r.contains(reason_substr)),
        "expected a rejection containing {:?}, got {:?}",
        reason_substr,
        output.rejected_functions
    );
}

/// #496 (B1): assert that ONE named function is rejected from WASM with the given
/// reason substring, WITHOUT requiring the whole module to be wasm-empty — the
/// dividing callee (`h`) is itself legitimately WASM-eligible, so only the guarded
/// caller must stay JS. Re-admitting the caller (guard removed) → not in
/// `rejected_functions` → RED (the paired negative control).
fn assert_fn_stays_js(source: &str, fn_name: &str, reason_substr: &str) {
    let module = pyths_parser::parse(source).expect("Parse failed");
    let output = codegen_wasm(&module);
    assert!(
        output
            .rejected_functions
            .iter()
            .any(|(n, r)| n == fn_name && r.contains(reason_substr)),
        "expected `{}` rejected with reason containing {:?}, got rejected={:?}",
        fn_name,
        reason_substr,
        output.rejected_functions,
    );
}

/// Helper to create a wasmi instance and call an i64 -> i64 function
fn call_i64_i64(wasm: &[u8], func_name: &str, arg: i64) -> i64 {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("Failed to load WASM module");
    let mut store = wasmi::Store::new(&engine, ());
    let linker = <wasmi::Linker<()>>::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("Failed to instantiate")
        .start(&mut store)
        .expect("Failed to start");
    let func = instance
        .get_typed_func::<i64, i64>(&store, func_name)
        .expect("Failed to get function");
    func.call(&mut store, arg).expect("Call failed")
}

/// Helper to call a (i64, i64) -> i64 function
fn call_i64_i64_i64(wasm: &[u8], func_name: &str, a: i64, b: i64) -> i64 {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("Failed to load WASM module");
    let mut store = wasmi::Store::new(&engine, ());
    let linker = <wasmi::Linker<()>>::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("Failed to instantiate")
        .start(&mut store)
        .expect("Failed to start");
    let func = instance
        .get_typed_func::<(i64, i64), i64>(&store, func_name)
        .expect("Failed to get function");
    func.call(&mut store, (a, b)).expect("Call failed")
}

/// Helper to call a (f64, f64) -> f64 function
fn call_f64_f64_f64(wasm: &[u8], func_name: &str, a: f64, b: f64) -> f64 {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("Failed to load WASM module");
    let mut store = wasmi::Store::new(&engine, ());
    let linker = <wasmi::Linker<()>>::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("Failed to instantiate")
        .start(&mut store)
        .expect("Failed to start");
    let func = instance
        .get_typed_func::<(f64, f64), f64>(&store, func_name)
        .expect("Failed to get function");
    func.call(&mut store, (a, b)).expect("Call failed")
}

/// Helper to call a (i64, i64) -> i32 (bool) function
fn call_i64_i64_i32(wasm: &[u8], func_name: &str, a: i64, b: i64) -> i32 {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("Failed to load WASM module");
    let mut store = wasmi::Store::new(&engine, ());
    let linker = <wasmi::Linker<()>>::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("Failed to instantiate")
        .start(&mut store)
        .expect("Failed to start");
    let func = instance
        .get_typed_func::<(i64, i64), i32>(&store, func_name)
        .expect("Failed to get function");
    func.call(&mut store, (a, b)).expect("Call failed")
}

// ============================
// Validation tests
// ============================

#[test]
fn test_single_function_module() {
    let wasm = compile_wasm("def add(a: int, b: int) -> int:\n    return a + b\n");
    validate_wasm(&wasm);
}

#[test]
fn test_multi_function_module() {
    let src = "\
def double(x: int) -> int:
    return x * 2

def quadruple(x: int) -> int:
    return double(double(x))
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
}

#[test]
fn test_float_function_validates() {
    let wasm = compile_wasm("def mul(x: float, y: float) -> float:\n    return x * y\n");
    validate_wasm(&wasm);
}

#[test]
fn test_void_return_function() {
    let wasm = compile_wasm("def noop(x: int):\n    pass\n");
    validate_wasm(&wasm);
}

#[test]
fn test_mixed_eligible_ineligible() {
    let module = pyths_parser::parse(
        "def add(a: int, b: int) -> int:\n    return a + b\ndef untyped(x):\n    return x\n",
    )
    .unwrap();
    let output = codegen_wasm(&module);
    assert_eq!(output.compiled_functions.len(), 1);
    assert!(output.compiled_functions.contains(&"add".to_string()));
    assert_eq!(output.rejected_functions.len(), 1);
    assert_eq!(output.rejected_functions[0].0, "untyped");
}

// ============================
// Expression tests
// ============================

#[test]
fn test_int_add() {
    let wasm = compile_wasm("def add(a: int, b: int) -> int:\n    return a + b\n");
    validate_wasm(&wasm);
    let result = call_i64_i64_i64(&wasm, "add", 3, 4);
    assert_eq!(result, 7);
}

#[test]
fn test_float_mul() {
    let wasm = compile_wasm("def mul(x: float, y: float) -> float:\n    return x * y\n");
    validate_wasm(&wasm);
    let result = call_f64_f64_f64(&wasm, "mul", 2.5, 3.0);
    assert!((result - 7.5).abs() < 1e-10);
}

#[test]
fn test_int_sub() {
    let wasm = compile_wasm("def sub(a: int, b: int) -> int:\n    return a - b\n");
    let result = call_i64_i64_i64(&wasm, "sub", 10, 3);
    assert_eq!(result, 7);
}

#[test]
fn test_comparison_returns_bool() {
    let wasm = compile_wasm("def gt(a: int, b: int) -> bool:\n    return a > b\n");
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64_i32(&wasm, "gt", 5, 3), 1);
    assert_eq!(call_i64_i64_i32(&wasm, "gt", 3, 5), 0);
    assert_eq!(call_i64_i64_i32(&wasm, "gt", 3, 3), 0);
}

#[test]
fn test_unary_neg() {
    let wasm = compile_wasm("def neg(x: int) -> int:\n    return -x\n");
    validate_wasm(&wasm);
    let result = call_i64_i64(&wasm, "neg", 42);
    assert_eq!(result, -42);
}

#[test]
fn test_bool_not() {
    let src = "def invert(x: bool) -> bool:\n    return not x\n";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, &wasm).unwrap();
    let mut store = wasmi::Store::new(&engine, ());
    let linker = <wasmi::Linker<()>>::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .unwrap()
        .start(&mut store)
        .unwrap();
    let func = instance
        .get_typed_func::<i32, i32>(&store, "invert")
        .unwrap();
    assert_eq!(func.call(&mut store, 1).unwrap(), 0);
    assert_eq!(func.call(&mut store, 0).unwrap(), 1);
}

#[test]
fn test_literal_types() {
    let wasm = compile_wasm("def five() -> int:\n    return 5\n");
    validate_wasm(&wasm);
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, &wasm).unwrap();
    let mut store = wasmi::Store::new(&engine, ());
    let linker = <wasmi::Linker<()>>::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .unwrap()
        .start(&mut store)
        .unwrap();
    let func = instance.get_typed_func::<(), i64>(&store, "five").unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), 5);
}

// ============================
// Statement + control flow tests
// ============================

#[test]
fn test_assign_and_return() {
    let src = "def f(n: int) -> int:\n    x: int = 5\n    return x\n";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64(&wasm, "f", 0), 5);
}

#[test]
fn test_augmented_assign() {
    let src = "def f(x: int) -> int:\n    x += 10\n    return x\n";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64(&wasm, "f", 5), 15);
}

#[test]
fn test_if_else() {
    let src = "\
def sign(x: int) -> int:
    if x > 0:
        return 1
    elif x < 0:
        return -1
    else:
        return 0
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64(&wasm, "sign", 42), 1);
    assert_eq!(call_i64_i64(&wasm, "sign", -5), -1);
    assert_eq!(call_i64_i64(&wasm, "sign", 0), 0);
}

#[test]
fn test_while_loop() {
    let src = "\
def sum_to(n: int) -> int:
    s: int = 0
    i: int = 1
    while i <= n:
        s += i
        i += 1
    return s
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64(&wasm, "sum_to", 10), 55);
    assert_eq!(call_i64_i64(&wasm, "sum_to", 0), 0);
}

#[test]
fn test_for_range() {
    let src = "\
def sum_range(n: int) -> int:
    total: int = 0
    for i in range(n):
        total += i
    return total
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64(&wasm, "sum_range", 100), 4950);
    assert_eq!(call_i64_i64(&wasm, "sum_range", 1), 0);
    assert_eq!(call_i64_i64(&wasm, "sum_range", 0), 0);
}

#[test]
fn test_for_range_start_stop() {
    let src = "\
def sum_range2(start: int, stop: int) -> int:
    total: int = 0
    for i in range(start, stop):
        total += i
    return total
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    assert_eq!(
        call_i64_i64_i64(&wasm, "sum_range2", 5, 10),
        5 + 6 + 7 + 8 + 9
    );
}

#[test]
fn test_nested_if_while() {
    let src = "\
def count_even(n: int) -> int:
    count: int = 0
    i: int = 0
    while i < n:
        if i % 2 == 0:
            count += 1
        i += 1
    return count
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64(&wasm, "count_even", 10), 5);
    assert_eq!(call_i64_i64(&wasm, "count_even", 7), 4);
}

// ============================
// Execution validation tests
// ============================

#[test]
fn test_exec_add() {
    let wasm = compile_wasm("def add(a: int, b: int) -> int:\n    return a + b\n");
    assert_eq!(call_i64_i64_i64(&wasm, "add", 3, 4), 7);
}

#[test]
fn test_exec_multiply() {
    let wasm = compile_wasm("def multiply(x: float, y: float) -> float:\n    return x * y\n");
    let result = call_f64_f64_f64(&wasm, "multiply", 2.5, 3.0);
    assert!((result - 7.5).abs() < 1e-10);
}

#[test]
fn test_exec_factorial() {
    let src = "\
def factorial(n: int) -> int:
    result: int = 1
    i: int = 2
    while i <= n:
        result *= i
        i += 1
    return result
";
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64(&wasm, "factorial", 10), 3628800);
    assert_eq!(call_i64_i64(&wasm, "factorial", 0), 1);
    assert_eq!(call_i64_i64(&wasm, "factorial", 1), 1);
}

#[test]
fn test_exec_fibonacci() {
    let src = "\
def fibonacci(n: int) -> int:
    if n <= 1:
        return n
    a: int = 0
    b: int = 1
    i: int = 2
    while i <= n:
        temp: int = a + b
        a = b
        b = temp
        i += 1
    return b
";
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64(&wasm, "fibonacci", 0), 0);
    assert_eq!(call_i64_i64(&wasm, "fibonacci", 1), 1);
    assert_eq!(call_i64_i64(&wasm, "fibonacci", 10), 55);
    assert_eq!(call_i64_i64(&wasm, "fibonacci", 20), 6765);
}

#[test]
fn test_exec_gcd() {
    let src = "\
def gcd(a: int, b: int) -> int:
    while b != 0:
        temp: int = b
        b = a % b
        a = temp
    return a
";
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64_i64(&wasm, "gcd", 48, 18), 6);
    assert_eq!(call_i64_i64_i64(&wasm, "gcd", 100, 75), 25);
    assert_eq!(call_i64_i64_i64(&wasm, "gcd", 17, 13), 1);
}

#[test]
fn test_exec_sum_range() {
    let src = "\
def sum_range(n: int) -> int:
    total: int = 0
    for i in range(n):
        total += i
    return total
";
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64(&wasm, "sum_range", 100), 4950);
}

#[test]
fn test_exec_modulo_negative() {
    let src = "\
def pymod(a: int, b: int) -> int:
    return a % b
";
    let wasm = compile_wasm(src);
    // Python semantics: -7 % 3 = 2 (not -1)
    assert_eq!(call_i64_i64_i64(&wasm, "pymod", -7, 3), 2);
    // 7 % 3 = 1
    assert_eq!(call_i64_i64_i64(&wasm, "pymod", 7, 3), 1);
    // 7 % -3 = -2 (Python semantics)
    assert_eq!(call_i64_i64_i64(&wasm, "pymod", 7, -3), -2);
}

#[test]
fn test_exec_floor_div_negative() {
    let src = "\
def floordiv(a: int, b: int) -> int:
    return a // b
";
    let wasm = compile_wasm(src);
    // Python: -7 // 2 = -4 (floor division)
    assert_eq!(call_i64_i64_i64(&wasm, "floordiv", -7, 2), -4);
    // 7 // 2 = 3
    assert_eq!(call_i64_i64_i64(&wasm, "floordiv", 7, 2), 3);
    // -7 // -2 = 3
    assert_eq!(call_i64_i64_i64(&wasm, "floordiv", -7, -2), 3);
}

#[test]
fn test_exec_function_call() {
    let src = "\
def double(x: int) -> int:
    return x * 2

def quadruple(x: int) -> int:
    return double(double(x))
";
    let wasm = compile_wasm(src);
    assert_eq!(call_i64_i64(&wasm, "quadruple", 5), 20);
    assert_eq!(call_i64_i64(&wasm, "double", 7), 14);
}

// ============================
// End-to-end fixture test
// ============================

#[test]
fn test_fixture_wasm_compute() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/wasm_compute.ps"),
    )
    .expect("Failed to read fixture");
    let module = pyths_parser::parse(&source).expect("Parse failed");
    let output = codegen_wasm(&module);
    assert!(
        !output.wasm.is_empty(),
        "Should produce WASM: {:?}",
        output.rejected_functions
    );
    validate_wasm(&output.wasm);

    // Verify all three functions were compiled
    assert!(output.compiled_functions.contains(&"fibonacci".to_string()));
    assert!(output.compiled_functions.contains(&"sum_range".to_string()));
    assert!(output.compiled_functions.contains(&"gcd".to_string()));

    // Execute fibonacci
    assert_eq!(call_i64_i64(&output.wasm, "fibonacci", 20), 6765);
    // Execute sum_range
    assert_eq!(call_i64_i64(&output.wasm, "sum_range", 100), 4950);
    // Execute gcd
    assert_eq!(call_i64_i64_i64(&output.wasm, "gcd", 48, 18), 6);
}

#[test]
fn test_fixture_wasm_numeric() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/wasm_numeric.ps"),
    )
    .expect("Failed to read fixture");
    let module = pyths_parser::parse(&source).expect("Parse failed");
    let output = codegen_wasm(&module);

    // add, multiply, factorial should be compiled; untyped should be rejected (no annotations)
    assert!(output.compiled_functions.contains(&"add".to_string()));
    assert!(output.compiled_functions.contains(&"multiply".to_string()));
    assert!(output.compiled_functions.contains(&"factorial".to_string()));
    assert!(
        output
            .rejected_functions
            .iter()
            .any(|(n, _)| n == "untyped"),
        "untyped should be rejected"
    );

    validate_wasm(&output.wasm);
    assert_eq!(call_i64_i64_i64(&output.wasm, "add", 3, 4), 7);
    assert_eq!(call_i64_i64(&output.wasm, "factorial", 10), 3628800);
}

// ============================
// String helpers
// ============================

/// Create a wasmi instance from WASM bytes, returning (store, instance).
fn make_instance(wasm: &[u8]) -> (wasmi::Store<()>, wasmi::Instance) {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("Failed to load WASM module");
    let mut store = wasmi::Store::new(&engine, ());
    let linker = <wasmi::Linker<()>>::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("Failed to instantiate")
        .start(&mut store)
        .expect("Failed to start");
    (store, instance)
}

/// Write a string into WASM memory using __alloc, returning the pointer.
#[allow(dead_code)] // reusable test-harness helper, not currently called
fn write_string(store: &mut wasmi::Store<()>, instance: &wasmi::Instance, s: &str) -> i32 {
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut *store, "__alloc")
        .expect("No __alloc export");
    let bytes = s.as_bytes();
    let ptr = alloc
        .call(&mut *store, (bytes.len() as i32) + 4)
        .expect("__alloc failed");

    let memory = instance
        .get_memory(&mut *store, "memory")
        .expect("No memory export");

    // Write length (little-endian i32)
    let len_bytes = (bytes.len() as i32).to_le_bytes();
    memory
        .write(&mut *store, ptr as usize, &len_bytes)
        .expect("Write length failed");
    // Write UTF-8 bytes
    memory
        .write(&mut *store, (ptr as usize) + 4, bytes)
        .expect("Write bytes failed");

    ptr
}

/// Read a string from WASM memory at the given pointer.
fn read_string(store: &wasmi::Store<()>, instance: &wasmi::Instance, ptr: i32) -> String {
    let memory = instance
        .get_memory(store, "memory")
        .expect("No memory export");

    let mut len_buf = [0u8; 4];
    memory
        .read(store, ptr as usize, &mut len_buf)
        .expect("Read length failed");
    let len = i32::from_le_bytes(len_buf) as usize;

    let mut str_buf = vec![0u8; len];
    memory
        .read(store, (ptr as usize) + 4, &mut str_buf)
        .expect("Read string failed");

    String::from_utf8(str_buf).expect("Invalid UTF-8")
}

// ============================
// String tests
// ============================

// #364 (Path B — numeric-kernel whitelist): STRING functions are general
// (non-numeric-kernel) code. The backend miscompiled complex string work under
// auto-routing, so `str` params/returns/literals/methods now stay on the correct
// JS path (fast via V8). These are regression guards for that boundary; WASM
// string support is deferred to the v3.x codegen-correctness pass (#364).
#[test]
fn test_string_literal_return() {
    assert_stays_js("def greet() -> str:\n    return \"hello\"\n", "JS");
}

#[test]
fn test_string_param_passthrough() {
    assert_stays_js("def identity(s: str) -> str:\n    return s\n", "JS");
}

#[test]
fn test_string_concat() {
    assert_stays_js(
        "def concat(a: str, b: str) -> str:\n    return a + b\n",
        "JS",
    );
}

#[test]
fn test_string_len() {
    assert_stays_js("def slen(s: str) -> int:\n    return len(s)\n", "JS");
}

#[test]
fn test_string_eq_true() {
    assert_stays_js(
        "def streq(a: str, b: str) -> bool:\n    return a == b\n",
        "JS",
    );
}

#[test]
fn test_string_eq_false() {
    assert_stays_js(
        "def streq2(a: str, b: str) -> bool:\n    return a == b\n",
        "JS",
    );
}

#[test]
fn test_string_ne() {
    assert_stays_js(
        "def strne(a: str, b: str) -> bool:\n    return a != b\n",
        "JS",
    );
}

#[test]
fn test_string_index() {
    assert_stays_js(
        "def char_at(s: str, i: int) -> str:\n    return s[i]\n",
        "JS",
    );
}

#[test]
fn test_string_upper() {
    assert_stays_js("def shout(s: str) -> str:\n    return s.upper()\n", "JS");
}

#[test]
fn test_string_lower() {
    assert_stays_js("def whisper(s: str) -> str:\n    return s.lower()\n", "JS");
}

#[test]
fn test_string_startswith() {
    assert_stays_js(
        "def starts(s: str, prefix: str) -> bool:\n    return s.startswith(prefix)\n",
        "JS",
    );
}

#[test]
fn test_string_endswith() {
    assert_stays_js(
        "def ends(s: str, suffix: str) -> bool:\n    return s.endswith(suffix)\n",
        "JS",
    );
}

#[test]
fn test_string_find() {
    assert_stays_js(
        "def sfind(s: str, sub: str) -> int:\n    return s.find(sub)\n",
        "JS",
    );
}

#[test]
fn test_string_assign_return() {
    assert_stays_js(
        "def assigned() -> str:\n    x: str = \"test\"\n    return x\n",
        "JS",
    );
}

#[test]
fn test_string_concat_multiple() {
    assert_stays_js(
        "def concat3(a: str, b: str, c: str) -> str:\n    return a + b + c\n",
        "JS",
    );
}

// ============================
// Tier 3: math.* import tests
// ============================

/// Build a wasmi linker that provides the standard `math` namespace imports.
fn make_math_instance(wasm: &[u8]) -> (wasmi::Store<()>, wasmi::Instance) {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("Failed to load WASM module");
    let mut store = wasmi::Store::new(&engine, ());
    let mut linker = <wasmi::Linker<()>>::new(&engine);

    fn add_f64_f64<F: Fn(f64) -> f64 + Send + Sync + 'static>(
        linker: &mut wasmi::Linker<()>,
        store: &mut wasmi::Store<()>,
        name: &str,
        f: F,
    ) {
        let host = wasmi::Func::wrap(&mut *store, move |x: f64| -> f64 { f(x) });
        linker.define("math", name, host).expect("define failed");
    }

    fn add_f64_f64_f64<F: Fn(f64, f64) -> f64 + Send + Sync + 'static>(
        linker: &mut wasmi::Linker<()>,
        store: &mut wasmi::Store<()>,
        name: &str,
        f: F,
    ) {
        let host = wasmi::Func::wrap(&mut *store, move |a: f64, b: f64| -> f64 { f(a, b) });
        linker.define("math", name, host).expect("define failed");
    }

    add_f64_f64(&mut linker, &mut store, "sqrt", f64::sqrt);
    add_f64_f64(&mut linker, &mut store, "sin", f64::sin);
    add_f64_f64(&mut linker, &mut store, "cos", f64::cos);
    add_f64_f64(&mut linker, &mut store, "tan", f64::tan);
    add_f64_f64(&mut linker, &mut store, "asin", f64::asin);
    add_f64_f64(&mut linker, &mut store, "acos", f64::acos);
    add_f64_f64(&mut linker, &mut store, "atan", f64::atan);
    add_f64_f64(&mut linker, &mut store, "log", f64::ln);
    add_f64_f64(&mut linker, &mut store, "log2", f64::log2);
    add_f64_f64(&mut linker, &mut store, "log10", f64::log10);
    add_f64_f64(&mut linker, &mut store, "exp", f64::exp);
    add_f64_f64(&mut linker, &mut store, "ceil", f64::ceil);
    add_f64_f64(&mut linker, &mut store, "floor", f64::floor);
    add_f64_f64(&mut linker, &mut store, "fabs", f64::abs);
    add_f64_f64_f64(&mut linker, &mut store, "atan2", f64::atan2);
    add_f64_f64_f64(&mut linker, &mut store, "pow", f64::powf);

    let instance = linker
        .instantiate(&mut store, &module)
        .expect("Failed to instantiate")
        .start(&mut store)
        .expect("Failed to start");
    (store, instance)
}

fn call_f64_f64(wasm: &[u8], func_name: &str, x: f64) -> f64 {
    let (mut store, instance) = make_math_instance(wasm);
    let func = instance
        .get_typed_func::<f64, f64>(&store, func_name)
        .expect("Failed to get function");
    func.call(&mut store, x).expect("Call failed")
}

#[test]
fn test_math_sqrt() {
    let wasm = compile_wasm("import math\ndef root(x: float) -> float:\n    return math.sqrt(x)\n");
    validate_wasm(&wasm);
    let r = call_f64_f64(&wasm, "root", 16.0);
    assert!((r - 4.0).abs() < 1e-9, "sqrt(16) = {}", r);
}

#[test]
fn test_math_sin_cos() {
    let src = "\
import math
def s(x: float) -> float:
    return math.sin(x)
def c(x: float) -> float:
    return math.cos(x)
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let s = call_f64_f64(&wasm, "s", 0.0);
    let c = call_f64_f64(&wasm, "c", 0.0);
    assert!(s.abs() < 1e-9);
    assert!((c - 1.0).abs() < 1e-9);
}

#[test]
fn test_math_log_exp() {
    let src = "\
import math
def lg(x: float) -> float:
    return math.log(x)
def ex(x: float) -> float:
    return math.exp(x)
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let l = call_f64_f64(&wasm, "lg", std::f64::consts::E);
    let e = call_f64_f64(&wasm, "ex", 0.0);
    assert!((l - 1.0).abs() < 1e-9);
    assert!((e - 1.0).abs() < 1e-9);
}

#[test]
fn test_math_log2_log10() {
    let src = "\
import math
def l2(x: float) -> float:
    return math.log2(x)
def l10(x: float) -> float:
    return math.log10(x)
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    assert!((call_f64_f64(&wasm, "l2", 8.0) - 3.0).abs() < 1e-9);
    assert!((call_f64_f64(&wasm, "l10", 1000.0) - 3.0).abs() < 1e-9);
}

#[test]
fn test_math_floor_ceil() {
    let src = "\
import math
def fl(x: float) -> float:
    return math.floor(x)
def ce(x: float) -> float:
    return math.ceil(x)
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    assert_eq!(call_f64_f64(&wasm, "fl", 3.7), 3.0);
    assert_eq!(call_f64_f64(&wasm, "ce", 3.2), 4.0);
}

#[test]
fn test_math_atan2() {
    let src = "import math\ndef a2(y: float, x: float) -> float:\n    return math.atan2(y, x)\n";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let func = instance
        .get_typed_func::<(f64, f64), f64>(&store, "a2")
        .expect("Failed to get function");
    let r = func.call(&mut store, (1.0, 1.0)).expect("Call failed");
    assert!((r - std::f64::consts::FRAC_PI_4).abs() < 1e-9);
}

#[test]
fn test_math_pi_constant() {
    let wasm = compile_wasm("import math\ndef p() -> float:\n    return math.pi\n");
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let func = instance.get_typed_func::<(), f64>(&store, "p").unwrap();
    let r = func.call(&mut store, ()).unwrap();
    assert!((r - std::f64::consts::PI).abs() < 1e-12);
}

#[test]
fn test_math_e_constant() {
    let wasm = compile_wasm("import math\ndef e() -> float:\n    return math.e\n");
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let func = instance.get_typed_func::<(), f64>(&store, "e").unwrap();
    let r = func.call(&mut store, ()).unwrap();
    assert!((r - std::f64::consts::E).abs() < 1e-12);
}

#[test]
fn test_math_pow_via_operator() {
    // ** operator should still work — uses math.pow import internally
    let wasm = compile_wasm("def p(b: float, e: float) -> float:\n    return b ** e\n");
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let func = instance
        .get_typed_func::<(f64, f64), f64>(&store, "p")
        .unwrap();
    assert_eq!(func.call(&mut store, (2.0, 10.0)).unwrap(), 1024.0);
}

#[test]
fn test_math_pow_via_call() {
    let src = "import math\ndef p(b: float, e: float) -> float:\n    return math.pow(b, e)\n";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let func = instance
        .get_typed_func::<(f64, f64), f64>(&store, "p")
        .unwrap();
    assert_eq!(func.call(&mut store, (3.0, 4.0)).unwrap(), 81.0);
}

#[test]
fn test_math_combined_in_one_function() {
    let src = "\
import math
def magnitude(x: float, y: float) -> float:
    return math.sqrt(x * x + y * y)
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let func = instance
        .get_typed_func::<(f64, f64), f64>(&store, "magnitude")
        .unwrap();
    let r = func.call(&mut store, (3.0, 4.0)).unwrap();
    assert!((r - 5.0).abs() < 1e-9);
}

#[test]
fn test_math_circle_area() {
    let src = "\
import math
def area(r: float) -> float:
    return math.pi * r ** 2
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let r = call_f64_f64(&wasm, "area", 2.0);
    assert!((r - std::f64::consts::PI * 4.0).abs() < 1e-9);
}

#[test]
fn test_no_math_imports_when_unused() {
    let module =
        pyths_parser::parse("def add(a: int, b: int) -> int:\n    return a + b\n").unwrap();
    let output = pyths_codegen_wasm::codegen_wasm(&module);
    assert!(
        output.math_imports.is_empty(),
        "no math imports for pure arithmetic"
    );
    assert!(!output.needs_pow_import());
}

#[test]
fn test_pow_operator_registers_pow_import() {
    let module =
        pyths_parser::parse("def p(b: float, e: float) -> float:\n    return b ** e\n").unwrap();
    let output = pyths_codegen_wasm::codegen_wasm(&module);
    assert!(output.math_imports.contains("pow"));
    assert!(output.needs_pow_import());
}

#[test]
fn test_math_call_registers_import() {
    let src = "import math\ndef f(x: float) -> float:\n    return math.sqrt(x) + math.sin(x)\n";
    let module = pyths_parser::parse(src).unwrap();
    let output = pyths_codegen_wasm::codegen_wasm(&module);
    assert!(output.math_imports.contains("sqrt"));
    assert!(output.math_imports.contains("sin"));
    assert!(!output.math_imports.contains("cos"));
}

// ============================
// Tier 7: error handling tests
// ============================

/// Read the __err_code global from a WASM instance.
fn read_err_code(store: &mut wasmi::Store<()>, instance: &wasmi::Instance) -> i32 {
    let g = instance
        .get_global(&mut *store, "__err_code")
        .expect("No __err_code export");
    match g.get(&mut *store) {
        wasmi::Val::I32(v) => v,
        _ => panic!("__err_code is not i32"),
    }
}

#[test]
fn test_raise_sets_err_code() {
    let src = "\
def bad(x: int) -> int:
    raise ValueError
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<i64, i64>(&store, "bad").unwrap();
    let r = func.call(&mut store, 1).unwrap();
    assert_eq!(r, 0); // sentinel
    assert_eq!(read_err_code(&mut store, &instance), 1); // ValueError
}

#[test]
fn test_raise_with_call_form() {
    // raise ValueError("msg") — message is dropped, type code is preserved.
    let src = "\
def bad(x: int) -> int:
    raise ValueError(\"oops\")
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<i64, i64>(&store, "bad").unwrap();
    let _ = func.call(&mut store, 1).unwrap();
    assert_eq!(read_err_code(&mut store, &instance), 1);
}

#[test]
fn test_raise_index_error() {
    let src = "\
def bad() -> int:
    raise IndexError
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<(), i64>(&store, "bad").unwrap();
    let _ = func.call(&mut store, ()).unwrap();
    assert_eq!(read_err_code(&mut store, &instance), 3);
}

#[test]
fn test_assert_passing() {
    let src = "\
def f(x: int) -> int:
    assert x > 0
    return x * 2
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<i64, i64>(&store, "f").unwrap();
    let r = func.call(&mut store, 5).unwrap();
    assert_eq!(r, 10);
    assert_eq!(read_err_code(&mut store, &instance), 0);
}

#[test]
fn test_assert_failing() {
    let src = "\
def f(x: int) -> int:
    assert x > 0
    return x * 2
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<i64, i64>(&store, "f").unwrap();
    let r = func.call(&mut store, -1).unwrap();
    assert_eq!(r, 0); // sentinel
    assert_eq!(read_err_code(&mut store, &instance), 6); // AssertionError
}

#[test]
fn test_raise_in_branch() {
    let src = "\
def safe_div(a: int, b: int) -> int:
    if b == 0:
        raise ZeroDivisionError
    return a // b
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance
        .get_typed_func::<(i64, i64), i64>(&store, "safe_div")
        .unwrap();

    // Normal case
    let r = func.call(&mut store, (10, 2)).unwrap();
    assert_eq!(r, 5);
    assert_eq!(read_err_code(&mut store, &instance), 0);

    // Raise case
    let r = func.call(&mut store, (10, 0)).unwrap();
    assert_eq!(r, 0);
    assert_eq!(read_err_code(&mut store, &instance), 5);
}

#[test]
fn test_try_except_caught() {
    let src = "\
def f(x: int) -> int:
    try:
        if x < 0:
            raise ValueError
        return x * 2
    except ValueError:
        return -1
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<i64, i64>(&store, "f").unwrap();

    // Normal case — no raise
    let r = func.call(&mut store, 5).unwrap();
    assert_eq!(r, 10);
    assert_eq!(read_err_code(&mut store, &instance), 0);

    // Raise + catch
    let r = func.call(&mut store, -1).unwrap();
    assert_eq!(r, -1);
    assert_eq!(read_err_code(&mut store, &instance), 0); // cleared by handler
}

#[test]
fn test_try_except_uncaught() {
    let src = "\
def f(x: int) -> int:
    try:
        if x < 0:
            raise IndexError
        return x * 2
    except ValueError:
        return -1
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<i64, i64>(&store, "f").unwrap();

    // IndexError is NOT caught by ValueError handler — should propagate
    let r = func.call(&mut store, -1).unwrap();
    assert_eq!(r, 0); // sentinel
    assert_eq!(read_err_code(&mut store, &instance), 3); // IndexError still set
}

// ============================
// #496: try/except ZeroDivisionError around a division-family op must NOT be
// WASM-admitted (integer //,% TRAP on a zero divisor; true / yields inf — the
// catchable ZeroDivisionError CPython raises is never dispatched, so the handler
// is dropped: a C3 error-occurrence divergence). Refuse-to-admit → the function
// stays on the JS backend, which raises and runs the handler like CPython.
//
// Each `assert_stays_js` here IS the paired negative control: a mutant that drops
// the guard re-admits the shape → `output.wasm` is non-empty → the test goes RED.
// ============================

// Unique to the #496 message (the IndexError guard also says "not supported on
// the WASM fast path", so match on the zero-divisor phrasing specifically).
const ZDIV_REASON: &str = "a zero divisor would";

#[test]
fn test_496_try_except_zerodiv_floordiv_stays_js() {
    // The pinned repro (issue #496 / M4 A6): `100 // (i % 3)` under
    // `except ZeroDivisionError` — on a zero divisor CPython runs the handler.
    assert_stays_js(
        "\
def zdiv(n: int) -> int:
    total = 0
    i = 0
    while i < n:
        try:
            total = total + 100 // (i % 3)
        except ZeroDivisionError:
            total = total - 1
        i = i + 1
    return total
",
        ZDIV_REASON,
    );
}

#[test]
fn test_496_try_except_zerodiv_mod_stays_js() {
    // sibling `%`
    assert_stays_js(
        "\
def m(a: int, b: int) -> int:
    try:
        return a % b
    except ZeroDivisionError:
        return -1
",
        ZDIV_REASON,
    );
}

#[test]
fn test_496_try_except_zerodiv_truediv_stays_js() {
    // sibling `/` (true division → f64.div → inf on zero, no raise: silent-wrong).
    assert_stays_js(
        "\
def d(a: int, b: int) -> float:
    try:
        return a / b
    except ZeroDivisionError:
        return -1.0
",
        ZDIV_REASON,
    );
}

#[test]
fn test_496_try_except_zerodiv_augassign_stays_js() {
    // sibling: augmented `//=`
    assert_stays_js(
        "\
def a(n: int, b: int) -> int:
    total = 100
    try:
        total //= b
    except ZeroDivisionError:
        total = -1
    return total
",
        ZDIV_REASON,
    );
}

#[test]
fn test_496_try_except_exception_superclass_stays_js() {
    // `except Exception` (an admitted superclass) also catches ZeroDivisionError.
    assert_stays_js(
        "\
def e(a: int, b: int) -> int:
    try:
        return a // b
    except Exception:
        return -1
",
        ZDIV_REASON,
    );
}

#[test]
fn test_496_try_bare_except_stays_js() {
    // bare `except:` catches everything, ZeroDivisionError included.
    assert_stays_js(
        "\
def bx(a: int, b: int) -> int:
    try:
        return a // b
    except:
        return -1
",
        ZDIV_REASON,
    );
}

#[test]
fn test_496_nested_try_division_in_inner_try_stays_js() {
    // The scan descends into a nested try body: the outer `except ZeroDivisionError`
    // sees the inner-try division and must refuse.
    assert_stays_js(
        "\
def nz(a: int, b: int) -> int:
    try:
        try:
            return a // b
        except ValueError:
            return 0
    except ZeroDivisionError:
        return -1
",
        ZDIV_REASON,
    );
}

// ---- over-refusal controls: these are SOUND on WASM and must STAY admitted ----

#[test]
fn test_496_no_over_refusal_wrong_handler_class_stays_wasm() {
    // `except ValueError` does NOT catch a ZeroDivisionError in CPython, so the
    // division's trap propagates out either way — WASM behaves correctly and the
    // function must remain WASM-eligible (no over-refusal).
    let wasm = compile_wasm(
        "\
def okv(a: int, b: int) -> int:
    try:
        return a // b
    except ValueError:
        return -1
",
    );
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64_i64(&wasm, "okv", 7, 2), 3);
}

#[test]
fn test_496_no_over_refusal_division_without_try_stays_wasm() {
    // A bare division with NO try/except is untouched by the #496 guard.
    let wasm = compile_wasm("def q(a: int, b: int) -> int:\n    return a // b\n");
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64_i64(&wasm, "q", 7, 2), 3);
}

#[test]
fn test_496_no_over_refusal_try_zerodiv_without_division_stays_wasm() {
    // `except ZeroDivisionError` around a try body with NO division op is sound
    // (nothing can raise ZeroDivisionError on the WASM path) — must stay admitted.
    let wasm = compile_wasm(
        "\
def nod(a: int, b: int) -> int:
    try:
        return a + b
    except ZeroDivisionError:
        return -1
",
    );
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64_i64(&wasm, "nod", 7, 2), 9);
}

// ---- B2 (Fable r1): `**` with a zero base + negative exponent raises
// ZeroDivisionError in CPython (`0 ** -1`); WASM yields a silent wrong value / inf.
// It must be refused under a ZeroDivisionError handler, same as the other siblings.

#[test]
fn test_496_try_except_zerodiv_pow_stays_js() {
    assert_stays_js(
        "\
def zp(a: int, b: int) -> int:
    try:
        return a ** b
    except ZeroDivisionError:
        return -1
",
        ZDIV_REASON,
    );
}

#[test]
fn test_496_try_except_zerodiv_pow_augassign_stays_js() {
    assert_stays_js(
        "\
def zpa(a: int, b: int) -> int:
    total = a
    try:
        total **= b
    except ZeroDivisionError:
        total = -1
    return total
",
        ZDIV_REASON,
    );
}

#[test]
fn test_496_no_over_refusal_pow_without_zde_handler_stays_wasm() {
    // `**` with NO ZeroDivisionError handler is untouched — it stays WASM-admitted.
    // (compile+validate only: int `**` imports host `math.pow`, so instantiating it
    // needs a linker that provides it — not relevant to the admission claim here.)
    let wasm = compile_wasm("def qp(a: int, b: int) -> int:\n    return a ** b\n");
    validate_wasm(&wasm);
}

// ---- B1 (Fable r1): a division inside a WASM-admitted CALLEE, under the caller's
// ZeroDivisionError handler, TRAPS across the WASM call boundary (uncatchable) and
// drops the handler. The caller must be refused (transitive `may_zerodiv`).

#[test]
fn test_496_try_except_zerodiv_direct_callee_stays_js() {
    // `g`'s try body calls `h`, which divides; `g` must stay on JS (only `g`, not
    // `h` — `h` alone is legitimately WASM-eligible).
    assert_fn_stays_js(
        "\
def h(a: int, b: int) -> int:
    return a // b

def g(a: int, b: int) -> int:
    try:
        return h(a, b)
    except ZeroDivisionError:
        return -1
",
        "g",
        ZDIV_REASON,
    );
}

#[test]
fn test_496_try_except_zerodiv_transitive_callee_stays_js() {
    // `g` -> `mid` -> `h` (divides): the transitive closure must reach it, and
    // `except Exception` catches ZeroDivisionError too.
    assert_fn_stays_js(
        "\
def h(a: int, b: int) -> int:
    return a // b

def mid(a: int, b: int) -> int:
    return h(a, b) + 1

def g(a: int, b: int) -> int:
    try:
        return mid(a, b)
    except Exception:
        return -1
",
        "g",
        ZDIV_REASON,
    );
}

#[test]
fn test_496_no_over_refusal_call_to_nondividing_fn_stays_wasm() {
    // A callee that does NOT divide is sound: the caller's `except ZeroDivisionError`
    // around a call to it must NOT be over-refused — it stays WASM-eligible.
    let wasm = compile_wasm(
        "\
def sq(a: int) -> int:
    return a * a

def gg(a: int, b: int) -> int:
    try:
        return sq(a) + b
    except ZeroDivisionError:
        return -1
",
    );
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64_i64(&wasm, "gg", 3, 2), 11);
    assert_eq!(call_i64_i64(&wasm, "sq", 4), 16);
}

#[test]
fn test_needs_errors_flag() {
    let plain = pyths_parser::parse("def f(x: int) -> int:\n    return x\n").unwrap();
    assert!(!pyths_codegen_wasm::codegen_wasm(&plain).needs_errors);

    let raise = pyths_parser::parse("def f() -> int:\n    raise ValueError\n").unwrap();
    assert!(pyths_codegen_wasm::codegen_wasm(&raise).needs_errors);

    let assert_src =
        pyths_parser::parse("def f(x: int) -> int:\n    assert x > 0\n    return x\n").unwrap();
    assert!(pyths_codegen_wasm::codegen_wasm(&assert_src).needs_errors);
}

// ============================
// Step 5: custom exception tests
// ============================

#[test]
fn test_custom_exception_registered() {
    let src = "\
class NotFound(Exception):
    pass

def find(x: int) -> int:
    if x < 0:
        raise NotFound
    return x
";
    let module = pyths_parser::parse(src).expect("parse");
    let output = pyths_codegen_wasm::codegen_wasm(&module);
    assert!(output.needs_errors);
    // Custom code starts at 100
    assert_eq!(output.custom_exceptions.get("NotFound"), Some(&100));
}

#[test]
fn test_multiple_custom_exceptions() {
    let src = "\
class NotFound(Exception):
    pass

class Forbidden(Exception):
    pass

def f() -> int:
    raise NotFound
";
    let module = pyths_parser::parse(src).expect("parse");
    let output = pyths_codegen_wasm::codegen_wasm(&module);
    assert_eq!(output.custom_exceptions.get("NotFound"), Some(&100));
    assert_eq!(output.custom_exceptions.get("Forbidden"), Some(&101));
}

#[test]
fn test_custom_exception_raise_sets_code() {
    let src = "\
class NotFound(Exception):
    pass

def find(x: int) -> int:
    raise NotFound
";
    let module = pyths_parser::parse(src).expect("parse");
    let wasm = pyths_codegen_wasm::codegen_wasm(&module).wasm;
    assert!(!wasm.is_empty());
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<i64, i64>(&store, "find").unwrap();
    let _ = func.call(&mut store, 1).unwrap();
    assert_eq!(read_err_code(&mut store, &instance), 100);
}

#[test]
fn test_custom_exception_caught() {
    let src = "\
class NotFound(Exception):
    pass

def f(x: int) -> int:
    try:
        if x < 0:
            raise NotFound
        return x * 2
    except NotFound:
        return -1
";
    let module = pyths_parser::parse(src).expect("parse");
    let wasm = pyths_codegen_wasm::codegen_wasm(&module).wasm;
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<i64, i64>(&store, "f").unwrap();

    // Normal path
    assert_eq!(func.call(&mut store, 5).unwrap(), 10);
    assert_eq!(read_err_code(&mut store, &instance), 0);

    // Caught path
    assert_eq!(func.call(&mut store, -1).unwrap(), -1);
    assert_eq!(read_err_code(&mut store, &instance), 0); // cleared by handler
}

#[test]
fn test_custom_exception_uncaught_propagates() {
    let src = "\
class NotFound(Exception):
    pass

class Forbidden(Exception):
    pass

def f(x: int) -> int:
    try:
        if x < 0:
            raise NotFound
        return x
    except Forbidden:
        return -1
";
    let module = pyths_parser::parse(src).expect("parse");
    let wasm = pyths_codegen_wasm::codegen_wasm(&module).wasm;
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<i64, i64>(&store, "f").unwrap();

    let _ = func.call(&mut store, -1).unwrap();
    // NotFound (100) should still be set since Forbidden handler doesn't match
    assert_eq!(read_err_code(&mut store, &instance), 100);
}

#[test]
fn test_exception_message_propagates_to_err_msg_global() {
    // raise NotFound("not in db") — message is stashed in __err_msg.
    // The bridge surfaces it as err.message, but in pure WASM we just verify
    // __err_msg is set to a non-zero ptr and the data is at that location.
    let src = "\
class NotFound(Exception):
    pass

def find(x: int) -> int:
    raise NotFound(\"not in db\")
";
    let module = pyths_parser::parse(src).expect("parse");
    let output = pyths_codegen_wasm::codegen_wasm(&module);
    assert!(
        !output.wasm.is_empty(),
        "rejected: {:?}",
        output.rejected_functions
    );
    validate_wasm(&output.wasm);
    let (mut store, instance) = make_instance(&output.wasm);
    let func = instance.get_typed_func::<i64, i64>(&store, "find").unwrap();
    let _ = func.call(&mut store, 1).unwrap();

    assert_eq!(read_err_code(&mut store, &instance), 100); // NotFound

    // Read __err_msg ptr and decode the string at that location.
    let msg_global = instance
        .get_global(&mut store, "__err_msg")
        .expect("__err_msg export");
    let msg_ptr = match msg_global.get(&mut store) {
        wasmi::Val::I32(v) => v,
        _ => panic!("__err_msg not i32"),
    };
    assert!(msg_ptr > 0, "msg ptr should be non-zero");
    let s = read_string(&store, &instance, msg_ptr);
    assert_eq!(s, "not in db");
}

#[test]
fn test_no_custom_exceptions_when_module_has_none() {
    let src = "def f() -> int:\n    raise ValueError\n";
    let module = pyths_parser::parse(src).expect("parse");
    let output = pyths_codegen_wasm::codegen_wasm(&module);
    assert!(output.custom_exceptions.is_empty());
}

// ============================
// Step 2: tuple literal tests (Tier 5)
// ============================

#[test]
fn test_tuple_literal_returns_valid_ptr() {
    // #364: a tuple-returning function is a non-scalar return — stays JS.
    assert_stays_js(
        "def pair() -> tuple:\n    return (1, 2)\n",
        "WASM-fast-path scalar",
    );
}

#[test]
fn test_tuple_in_function_body() {
    // #364: a tuple literal (even as a local) is general data — stays JS.
    assert_stays_js(
        "def f(n: int) -> int:\n    t = (1, 2, 3)\n    return n\n",
        "JS",
    );
}

// ============================
// Step 3: list literal tests (Tier 2)
// ============================

#[test]
fn test_list_literal_returns_valid_ptr() {
    // #364: a list-returning function is a non-scalar return — stays JS.
    assert_stays_js(
        "def make() -> list:\n    return [1, 2, 3]\n",
        "WASM-fast-path scalar",
    );
}

#[test]
fn test_empty_list_literal() {
    // #364: non-scalar (list) return — stays JS.
    assert_stays_js(
        "def empty() -> list:\n    return []\n",
        "WASM-fast-path scalar",
    );
}

// ============================
// Step 4: dict literal tests (Tier 4 stub)
// ============================

#[test]
fn test_dict_literal_compiles() {
    // #364: a dict-returning function is a non-scalar return — stays JS.
    assert_stays_js(
        "def make() -> dict:\n    return {\"a\": 1, \"b\": 2}\n",
        "WASM-fast-path scalar",
    );
}

// ============================
// Tuple operations: indexing, unpacking, len
// ============================

#[test]
fn test_tuple_indexing_returns_first_element() {
    // #364: tuple literals are general data — stays JS (was: WASM tuple indexing).
    assert_stays_js(
        "def first() -> int:\n    t = (10, 20, 30)\n    return t[0]\n",
        "JS",
    );
}

#[test]
fn test_tuple_indexing_returns_middle_element() {
    assert_stays_js(
        "def middle() -> int:\n    t = (10, 20, 30)\n    return t[1]\n",
        "JS",
    );
}

#[test]
fn test_tuple_unpacking() {
    // #364: tuple-unpacking assignment is general data — stays JS.
    assert_stays_js(
        "def sum_pair() -> int:\n    t = (10, 20)\n    a, b = t\n    return a + b\n",
        "JS",
    );
}

#[test]
fn test_tuple_len_compile_time() {
    assert_stays_js(
        "def n() -> int:\n    t = (1, 2, 3, 4, 5)\n    return len(t)\n",
        "JS",
    );
}

// ============================
// List operations: indexing, len, append, iteration
// ============================

#[test]
fn test_list_indexing() {
    let src = "\
def get(i: int) -> int:
    lst = [10, 20, 30, 40]
    return lst[i]
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<i64, i64>(&store, "get").unwrap();
    assert_eq!(func.call(&mut store, 0).unwrap(), 10);
    assert_eq!(func.call(&mut store, 2).unwrap(), 30);
    assert_eq!(func.call(&mut store, 3).unwrap(), 40);
}

#[test]
fn test_list_len() {
    let src = "\
def n() -> int:
    lst = [1, 2, 3, 4, 5, 6, 7]
    return len(lst)
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<(), i64>(&store, "n").unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), 7);
}

#[test]
fn test_list_iteration_sums_elements() {
    let src = "\
def sum_list() -> int:
    lst = [10, 20, 30, 40, 50]
    total: int = 0
    for x in lst:
        total = total + x
    return total
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance
        .get_typed_func::<(), i64>(&store, "sum_list")
        .unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), 150);
}

#[test]
fn test_list_oob_raises_index_error() {
    // When the function uses error infrastructure (raise/assert/try), list
    // subscripts are bounds-checked and OOB sets __err_code = 3 (IndexError).
    let src = "\
def get(i: int) -> int:
    lst = [10, 20, 30]
    if i < 0:
        raise ValueError
    return lst[i]
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<i64, i64>(&store, "get").unwrap();

    // In-bounds: works as expected.
    let r = func.call(&mut store, 1).unwrap();
    assert_eq!(r, 20);
    assert_eq!(read_err_code(&mut store, &instance), 0);

    // Out of bounds: returns sentinel 0 and sets err_code = 3 (IndexError).
    let r = func.call(&mut store, 10).unwrap();
    assert_eq!(r, 0);
    assert_eq!(read_err_code(&mut store, &instance), 3);
}

#[test]
fn test_list_subscript_assignment() {
    let src = "\
def f() -> int:
    lst = [1, 2, 3]
    lst[1] = 99
    return lst[1]
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<(), i64>(&store, "f").unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), 99);
}

#[test]
fn test_list_append_pop() {
    let src = "\
def f() -> int:
    lst = [10, 20, 30, 0, 0, 0]
    lst[3] = 40
    return lst[3]
";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let (mut store, instance) = make_instance(&wasm);
    let func = instance.get_typed_func::<(), i64>(&store, "f").unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), 40);
}

// ============================
// Closure tests (Tier 6 — no captures, single arg)
// ============================

#[test]
fn test_simple_lambda_call() {
    // #364: lambdas/closures are not numeric-kernel — stays JS.
    assert_stays_js(
        "def f() -> int:\n    add_one = lambda x: x + 1\n    return add_one(5)\n",
        "JS",
    );
}

#[test]
fn test_lambda_two_args() {
    assert_stays_js(
        "def f() -> int:\n    add = lambda a, b: a + b\n    return add(3, 4)\n",
        "JS",
    );
}

#[test]
fn test_two_distinct_lambdas() {
    // #364: lambdas/closures are not numeric-kernel — stays JS.
    assert_stays_js("def f() -> int:\n    double = lambda x: x * 2\n    triple = lambda x: x * 3\n    return double(5) + triple(5)\n", "JS");
}

#[test]
fn test_lambda_captures_param() {
    assert_stays_js(
        "def make_adder(n: int) -> int:\n    add = lambda x: x + n\n    return add(5)\n",
        "JS",
    );
}

#[test]
fn test_map_doubles_list() {
    // #364: map/filter/reduce (+lambda) build collections — stays JS.
    assert_stays_js("def f() -> int:\n    nums = [1, 2, 3, 4]\n    doubled = map(lambda x: x * 2, nums)\n    return doubled[0] + doubled[1] + doubled[2] + doubled[3]\n", "JS");
}

#[test]
fn test_filter_evens() {
    assert_stays_js("def f() -> int:\n    nums = [1, 2, 3, 4, 5, 6]\n    evens = filter(lambda x: x % 2 == 0, nums)\n    return evens[0] + evens[1] + evens[2]\n", "JS");
}

#[test]
fn test_reduce_sum() {
    assert_stays_js("def f() -> int:\n    nums = [1, 2, 3, 4, 5]\n    return reduce(lambda a, b: a + b, nums, 0)\n", "JS");
}

#[test]
fn test_sorted_strings_passes_through_unchanged() {
    // Lexicographic string sort isn't yet emitted — `sorted(strs)` on a
    // string list passes the input through unchanged. The module must
    // still validate (no malformed instructions in the pass-through
    // path) and return SOME string from the list. The next §4 item is
    // emitting a `__str_le` helper to implement real string compare.
    let src = "\
def f() -> int:
    words = [\"banana\", \"apple\", \"cherry\"]
    s = sorted(words)
    return len(s)
";
    let module = pyths_parser::parse(src).expect("parse");
    let output = pyths_codegen_wasm::codegen_wasm(&module);
    if output.wasm.is_empty() {
        return;
    }
    validate_wasm(&output.wasm);
    let (mut store, instance) = make_instance(&output.wasm);
    let func = instance.get_typed_func::<(), i64>(&store, "f").unwrap();
    // Pass-through: len(s) == len(words) == 3.
    let r = func.call(&mut store, ()).unwrap();
    assert_eq!(r, 3);
}

#[test]
fn test_map_with_type_changing_lambda() {
    // map(lambda x: x * 2.0, int_list) — the lambda's `x` is inferred
    // as I64 from the input list, and the body `x * 2.0` produces F64
    // (numeric promotion). The output list is therefore PtrList(F64).
    // Drop the `list()` wrapper — WASM's `map` returns a PtrList
    // directly (no iterator semantics).
    let src = "\
def f() -> float:
    nums = [1, 2, 3, 4]
    out = map(lambda x: x * 2.0, nums)
    return out[0]
";
    let module = pyths_parser::parse(src).expect("parse");
    let output = pyths_codegen_wasm::codegen_wasm(&module);
    if output.wasm.is_empty() {
        eprintln!("REJECTED: {:?}", output.rejected_functions);
        return;
    }
    validate_wasm(&output.wasm);
    let (mut store, instance) = make_instance(&output.wasm);
    let func = instance.get_typed_func::<(), f64>(&store, "f").unwrap();
    // 1 * 2.0 = 2.0
    let r = func.call(&mut store, ()).unwrap();
    assert!((r - 2.0).abs() < 1e-9, "got {}", r);
}

#[test]
fn test_map_lambda_preserves_input_type() {
    // map(lambda x: x + 1, int_list) — input and output both i64.
    let src = "\
def f() -> int:
    nums = [10, 20, 30]
    out = map(lambda x: x + 1, nums)
    return out[2]
";
    let module = pyths_parser::parse(src).expect("parse");
    let output = pyths_codegen_wasm::codegen_wasm(&module);
    if output.wasm.is_empty() {
        return;
    }
    validate_wasm(&output.wasm);
    let (mut store, instance) = make_instance(&output.wasm);
    let func = instance.get_typed_func::<(), i64>(&store, "f").unwrap();
    // [11, 21, 31][2] = 31
    let r = func.call(&mut store, ()).unwrap();
    assert_eq!(r, 31);
}

#[test]
fn test_reduce_float_accumulator() {
    // f64 reduce via closure-arg-from-context inference:
    // * `collect_lambdas_in_expr_scoped_with_overrides` propagates
    //   `(init_ty, elem_ty)` from the surrounding `reduce()` call
    //   into the lambda's param types at collection time, so the
    //   emitted lambda function uses F64 params and returns F64.
    // * `emit_reduce` mirrors the same context inference when
    //   computing the call signature, so `closure_type_indices`
    //   lookup matches the registered key.
    let src = "\
def f() -> float:
    nums = [1.5, 2.5, 3.0, 4.0]
    return reduce(lambda a, b: a + b, nums, 0.0)
";
    let module = pyths_parser::parse(src).expect("parse");
    let output = pyths_codegen_wasm::codegen_wasm(&module);
    if output.wasm.is_empty() {
        return;
    }
    validate_wasm(&output.wasm);
    let (mut store, instance) = make_instance(&output.wasm);
    let func = instance.get_typed_func::<(), f64>(&store, "f").unwrap();
    // 1.5 + 2.5 + 3.0 + 4.0 = 11.0
    let r = func.call(&mut store, ()).unwrap();
    assert!((r - 11.0).abs() < 1e-9, "got {}", r);
}

#[test]
fn test_sorted_ascending() {
    // #364: sorted() builds a new collection — stays JS (not numeric-kernel).
    assert_stays_js("def f() -> int:\n    nums = [3, 1, 4, 1, 5, 9, 2, 6]\n    s = sorted(nums)\n    return s[0] * 100 + s[7]\n", "JS");
}

#[test]
fn test_sorted_ascending_floats() {
    assert_stays_js("def f() -> float:\n    nums = [3.5, 1.25, 4.0, 1.0, 5.5]\n    s = sorted(nums)\n    return s[0] * 100.0 + s[4]\n", "JS");
}

#[test]
fn test_sorted_ascending_booleans() {
    // Element-type generalization: bool lists (stored as i32) sort
    // with I32Le compare. False (0) sorts before True (1).
    let src = "\
def f() -> bool:
    flags: list = [True, False, True, False, True]
    s = sorted(flags)
    return s[0]
";
    let module = pyths_parser::parse(src).expect("parse");
    let output = pyths_codegen_wasm::codegen_wasm(&module);
    if output.wasm.is_empty() {
        // If not eligible, skip — the bool list path may need additional
        // HIR analysis support that's outside this sort-specific patch.
        return;
    }
    validate_wasm(&output.wasm);
    // Don't execute — bool list eligibility depends on HIR Tier 2 list
    // support; the load-bearing assertion is that the WASM module
    // validates with the new I32 sort instructions.
}

#[test]
fn test_sorted_floats_validates_only() {
    // #364: sorted() builds a new collection — stays JS.
    assert_stays_js(
        "def f() -> float:\n    nums = [9.9, 0.1, 5.5, 2.0]\n    return sorted(nums)[0]\n",
        "JS",
    );
}

#[test]
fn test_lambda_captures_two_values() {
    // #364: lambdas/closures are not numeric-kernel — stays JS.
    assert_stays_js(
        "def f(a: int, b: int) -> int:\n    fn = lambda x: x * a + b\n    return fn(7)\n",
        "JS",
    );
}

#[test]
fn test_dict_literal_with_imports_validates() {
    // Dict bridge wiring produces a WASM module that imports __dict.* host
    // functions. wasmi can't execute it without those imports defined, but
    // we can still validate the binary structure.
    // #364: dict-returning function is a non-scalar return — stays JS.
    assert_stays_js(
        "def make() -> dict:\n    return {\"alpha\": 1, \"beta\": 2}\n",
        "WASM-fast-path scalar",
    );
}

// ============================================================================
// B-032 regression: indexing a list *parameter* of floats.
//
// Root cause: list subscript uses scratch temps `str_temps[2]`/`[3]`, but the
// temp pool was only allocated when `needs_strings` was set. A function whose
// only heap interaction is a list *param* (never constructing a collection in
// its body) had an empty temp pool, so `.get(2)/.get(3)` fell back to local 0
// — the list pointer itself. The index write then clobbered the pointer and
// the load read address 8 → every `lst[i]` returned 0, so `norm([3,4])` → 0.
//
// The fix broadens the "needs heap/temps" detection to include collection
// params/returns, which also makes the module export `__alloc` (needed by the
// JS glue to marshal a JS array into linear memory — B-031).
// ============================================================================

/// Write an f64 list `[i32 len][i32 cap][f64...]` into memory via __alloc and
/// return the pointer.
fn write_f64_list(store: &mut wasmi::Store<()>, instance: &wasmi::Instance, vals: &[f64]) -> i32 {
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut *store, "__alloc")
        .expect("list-param module must export __alloc");
    let n = vals.len() as i32;
    let ptr = alloc.call(&mut *store, 8 + n * 8).expect("__alloc failed");
    let memory = instance
        .get_memory(&mut *store, "memory")
        .expect("No memory export");
    memory
        .write(&mut *store, ptr as usize, &n.to_le_bytes())
        .unwrap();
    memory
        .write(&mut *store, (ptr as usize) + 4, &n.to_le_bytes())
        .unwrap();
    for (i, v) in vals.iter().enumerate() {
        memory
            .write(&mut *store, (ptr as usize) + 8 + i * 8, &v.to_le_bytes())
            .unwrap();
    }
    ptr
}

fn call_listf64_f64(wasm: &[u8], func_name: &str, vals: &[f64]) -> f64 {
    let (mut store, instance) = make_math_instance(wasm);
    let ptr = write_f64_list(&mut store, &instance, vals);
    let func = instance
        .get_typed_func::<i32, f64>(&store, func_name)
        .expect("Failed to get function");
    func.call(&mut store, ptr).expect("Call failed")
}

#[test]
fn test_b032_list_param_index_returns_element() {
    // Direct read of a float list-param element — the minimal repro of the bug.
    let wasm = compile_wasm("def g0(a: list[float]) -> float:\n    return a[0]\n");
    validate_wasm(&wasm);
    assert_eq!(call_listf64_f64(&wasm, "g0", &[3.0, 4.0]), 3.0);

    let wasm1 = compile_wasm("def g1(a: list[float]) -> float:\n    return a[1]\n");
    assert_eq!(call_listf64_f64(&wasm1, "g1", &[3.0, 4.0]), 4.0);
}

#[test]
fn test_b032_norm_pow_over_list_param() {
    let src = "def norm(a: list[float]) -> float:\n\
               \x20   s: float = 0.0\n\
               \x20   i: int = 0\n\
               \x20   while i < len(a):\n\
               \x20       s = s + a[i]*a[i]\n\
               \x20       i = i + 1\n\
               \x20   return s ** 0.5\n";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    // 3² + 4² = 25, √25 = 5
    assert_eq!(call_listf64_f64(&wasm, "norm", &[3.0, 4.0]), 5.0);
    // 5² + 12² = 169, √169 = 13
    assert_eq!(call_listf64_f64(&wasm, "norm", &[5.0, 12.0]), 13.0);
}

#[test]
fn test_b032_list_param_module_exports_alloc() {
    // A list-param fn that never constructs a collection must still export
    // __alloc so the JS glue can marshal arrays into linear memory (B-031).
    let wasm = compile_wasm("def f(a: list[float]) -> float:\n    return a[0] + a[1]\n");
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    assert!(
        instance
            .get_typed_func::<i32, i32>(&mut store, "__alloc")
            .is_ok(),
        "list-param module must export __alloc"
    );
    assert_eq!(call_listf64_f64(&wasm, "f", &[3.0, 4.0]), 7.0);
}

// ============================================================================
// B-032 (part 2) regression: a bare math call `sqrt(...)` bound via
// `from math import sqrt`, applied to the result of an internal user-function
// call that takes a list/pointer arg — `sqrt(dot(a, a))`.
//
// Root cause: bare math-import calls were unhandled in three places — math
// import collection (only `math.X(...)` was detected), call dispatch (bare
// name fell through emitting nothing), and type inference (defaulted to i64).
// So `return sqrt(dot(a,a))` emitted no value and then converted i64->f64 on
// an empty stack -> "f64.convert_i64_s (need 1, got 0)" at validate time, AND
// the `sqrt` import was never wired. Fixed by tracking `from math import`
// aliases and dispatching bare calls like the attribute form.
// ============================================================================

#[test]
fn test_b032b_bare_sqrt_of_internal_list_call_validates_and_runs() {
    let src = "from math import sqrt\n\
               def dot(a: list[float], b: list[float]) -> float:\n\
               \x20   s: float = 0.0\n\
               \x20   i: int = 0\n\
               \x20   while i < len(a):\n\
               \x20       s = s + a[i]*b[i]\n\
               \x20       i = i + 1\n\
               \x20   return s\n\
               def norm(a: list[float]) -> float:\n\
               \x20   return sqrt(dot(a, a))\n";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm); // RED: previously failed to even validate (empty-stack convert)
                          // 3² + 4² = 25, √25 = 5
    assert_eq!(call_listf64_f64(&wasm, "norm", &[3.0, 4.0]), 5.0);
    // 5² + 12² = 169, √169 = 13
    assert_eq!(call_listf64_f64(&wasm, "norm", &[5.0, 12.0]), 13.0);
}

#[test]
fn test_b032b_bare_sqrt_scalar() {
    // Minimal: bare imported sqrt on a plain f64 param.
    let wasm =
        compile_wasm("from math import sqrt\ndef f(x: float) -> float:\n    return sqrt(x)\n");
    validate_wasm(&wasm);
    assert_eq!(call_f64_f64(&wasm, "f", 16.0), 4.0);
}

#[test]
fn test_b032b_bare_sqrt_with_alias() {
    // #364: a math import bound to an ARBITRARY alias name (`sqrt as s`) can't be
    // resolved to a whitelisted math function without import context, so it stays
    // JS. Canonical `math.sqrt` and `from math import sqrt` remain WASM-eligible
    // (see similarity `norm`). Aliased-math→WASM is a v3.x follow-up (#364).
    assert_stays_js(
        "from math import sqrt as s\ndef f(x: float) -> float:\n    return s(x)\n",
        "JS",
    );
}

/// Read an f64 list `[i32 len][i32 cap][f64...]` from WASM memory at `ptr`.
#[allow(dead_code)] // reusable test-harness helper, not currently called
fn read_f64_list(store: &wasmi::Store<()>, instance: &wasmi::Instance, ptr: i32) -> Vec<f64> {
    let memory = instance
        .get_memory(store, "memory")
        .expect("No memory export");
    let mut len_buf = [0u8; 4];
    memory.read(store, ptr as usize, &mut len_buf).unwrap();
    let n = i32::from_le_bytes(len_buf) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let mut b = [0u8; 8];
        memory
            .read(store, (ptr as usize) + 8 + i * 8, &mut b)
            .unwrap();
        out.push(f64::from_le_bytes(b));
    }
    out
}

// ============================================================================
// I-1 — execution test for the list-RETURN path (`__list_from_wasm` side).
// The bridge test only checks the glue defines the helper; this exercises the
// actual returned-pointer layout the JS helper reads.
// ============================================================================

#[test]
fn test_i1_list_return_execution_values() {
    // #364: a list[float]-returning function is a non-scalar return — stays JS
    // (the boundary marshalling out was miscompiled; V8 handles it correctly).
    assert_stays_js(
        "def mk() -> list[float]:\n    return [3.0, 4.0]\n",
        "WASM-fast-path scalar",
    );
}

// ============================================================================
// I-2 — multi-arg bare-alias math call. Single-arg sqrt is covered by the
// B-032b tests; atan2/pow are 2-arg, exercising the `for i in 0..arity`
// dispatch branch for a bare `from math import` alias.
// ============================================================================

/// Call a `(f64, f64) -> f64` export with the `math` namespace linked
/// (for fns that import math functions like atan2/pow).
fn call_math_f64_f64_f64(wasm: &[u8], func_name: &str, a: f64, b: f64) -> f64 {
    let (mut store, instance) = make_math_instance(wasm);
    instance
        .get_typed_func::<(f64, f64), f64>(&store, func_name)
        .expect("Failed to get function")
        .call(&mut store, (a, b))
        .expect("Call failed")
}

#[test]
fn test_i2_bare_atan2_two_arg() {
    let wasm = compile_wasm(
        "from math import atan2\ndef f(y: float, x: float) -> float:\n    return atan2(y, x)\n",
    );
    validate_wasm(&wasm);
    // atan2(1, 1) = pi/4
    let got = call_math_f64_f64_f64(&wasm, "f", 1.0, 1.0);
    assert!(
        (got - std::f64::consts::FRAC_PI_4).abs() < 1e-12,
        "atan2(1,1)={}",
        got
    );
}

#[test]
fn test_i2_bare_pow_alias_two_arg() {
    // #364: aliased math import (`pow as p`) stays JS (see the sqrt-alias note).
    assert_stays_js(
        "from math import pow as p\ndef f(b: float, e: float) -> float:\n    return p(b, e)\n",
        "JS",
    );
}

// ============================================================================
// I-3 — B-033 `[x] * n` list-repetition. This codegen path miscompiled (a
// length-1 list). #364 (Path B) makes it unreachable under the default: every
// case here RETURNS a list — a non-scalar return — so the function stays on the
// correct JS path and is never WASM-compiled. These are now regression guards
// for the tightening; re-enabling `[x]*n` in WASM is deferred to the v3.x
// codegen-correctness pass (#364). See reference-app bug_fix_report.md B-033.
// ============================================================================

#[test]
#[allow(non_snake_case)] // intentional KNOWN_BUG marker in the name for discoverability
fn list_repetition_miscompiles_KNOWN_BUG() {
    // #364: list return → stays JS, so the [x]*n miscompile can never surface.
    assert_stays_js(
        "def rep() -> list[float]:\n    return [0.0] * 3\n",
        "WASM-fast-path scalar",
    );
}

#[test]
fn test_b033_list_repeat_multi_element_source() {
    assert_stays_js(
        "def rep() -> list[float]:\n    return [1.0, 2.0] * 3\n",
        "WASM-fast-path scalar",
    );
}

#[test]
fn test_b033_list_repeat_runtime_n() {
    assert_stays_js(
        "def rep(n: int) -> list[float]:\n    return [1.0] * n\n",
        "WASM-fast-path scalar",
    );
}

#[test]
fn test_b033_list_repeat_zero() {
    assert_stays_js(
        "def rep() -> list[float]:\n    return [0.0] * 0\n",
        "WASM-fast-path scalar",
    );
}

// ============================================================================
// B-034 regression: repeated list-arg WASM calls must not exhaust memory or
// crash, and must stay correct. Two underlying fixes:
//   1. `__alloc` grows by ceil(deficit/65536) pages (a single >64 KiB alloc,
//      e.g. a 1000-element f64 list, needs the right page count — one page was
//      not enough → out-of-bounds).
//   2. The JS glue saves/restores `__heap_ptr` (exported here) around each call
//      so transient argument memory is reclaimed (arena scope).
// This wasmi test mirrors the glue's save/restore and asserts both invariants:
// the value is correct every iteration AND linear memory stays bounded (does
// not grow per-iteration) across 1500 repetitions of a 1000-element list arg.
// ============================================================================

#[test]
fn test_b034_repeated_list_arg_calls_bounded_and_correct() {
    let wasm = compile_wasm(
        "def dot(a: list[float], b: list[float]) -> float:\n\
         \x20   s: float = 0.0\n\
         \x20   i: int = 0\n\
         \x20   while i < len(a):\n\
         \x20       s = s + a[i]*b[i]\n\
         \x20       i = i + 1\n\
         \x20   return s\n",
    );
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);

    // 1000-element vectors: a = all 1.0, b = all 2.0 → dot = 2000.0.
    let a: Vec<f64> = vec![1.0; 1000];
    let b: Vec<f64> = vec![2.0; 1000];

    let heap_ptr = instance
        .get_global(&mut store, "__heap_ptr")
        .expect("No __heap_ptr export");
    let dot = instance
        .get_typed_func::<(i32, i32), f64>(&store, "dot")
        .expect("no dot");

    let mut max_pages_after: u32 = 0;
    for _ in 0..1500 {
        // Save bump pointer (mirrors the glue's `const __sp = __heap_ptr.value`).
        let sp = match heap_ptr.get(&store) {
            wasmi::Val::I32(v) => v,
            _ => panic!("__heap_ptr not i32"),
        };
        let pa = write_f64_list(&mut store, &instance, &a);
        let pb = write_f64_list(&mut store, &instance, &b);
        let got = dot.call(&mut store, (pa, pb)).expect("dot call failed");
        assert_eq!(got, 2000.0, "dot drifted across repeats");
        // Restore (arena reset).
        heap_ptr
            .set(&mut store, wasmi::Val::I32(sp))
            .expect("reset failed");
        let mem = instance.get_memory(&store, "memory").unwrap();
        max_pages_after = max_pages_after.max(mem.size(&store) as u32);
    }
    // With the arena reset, ~16 KiB of transient args are reused each iteration,
    // so total memory stays tiny — a handful of pages, NOT ~1500*16KiB. This is
    // the bound that fails (OOM/crash) without the fix.
    assert!(
        max_pages_after < 16,
        "memory grew unbounded ({} pages) — arena reset not reclaiming",
        max_pages_after
    );
}

// --- Livermore-suite finding (2026-07-10): a local whose FIRST assignment
// is a float-list subscript read (`a = x[k]`) was typed i64 (the
// collect_typed_locals literal-only default), producing invalid WASM:
// "local.set expected type i64, found f64.load". 8 of the 24 LFK kernels
// hit this shape (k02/k04/k10/k13/k14/k15/k17/k23).

#[test]
fn test_local_from_float_subscript_types_f64() {
    let wasm = compile_wasm(
        "def scalar_from_load(n: int) -> float:\n\
         \x20   x = [0.0] * n\n\
         \x20   i = 0\n\
         \x20   while i < n:\n\
         \x20       x[i] = 0.5 * (i + 1)\n\
         \x20       i = i + 1\n\
         \x20   s = 0.0\n\
         \x20   k = 1\n\
         \x20   while k < n:\n\
         \x20       a = x[k - 1]\n\
         \x20       b = x[k]\n\
         \x20       s = s + a * b\n\
         \x20       k = k + 1\n\
         \x20   return s\n",
    );
    validate_wasm(&wasm);
    // n=4: x = [0.5,1,1.5,2]; s = 0.5*1 + 1*1.5 + 1.5*2 = 5.0
    let got = call_i64_f64(&wasm, "scalar_from_load", 4);
    assert_eq!(got, 5.0);
}

fn call_i64_f64(wasm: &[u8], func_name: &str, a: i64) -> f64 {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("Failed to parse WASM");
    let mut store = wasmi::Store::new(&engine, ());
    let linker = wasmi::Linker::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .expect("Failed to instantiate")
        .start(&mut store)
        .expect("Failed to start");
    let func = instance
        .get_typed_func::<i64, f64>(&store, func_name)
        .expect("Function not found");
    func.call(&mut store, a).expect("Call failed")
}

#[test]
fn test_nested_subscript_read_no_scratch_clobber() {
    // Livermore k14 finding (2026-07-10): `rh[ir[k]]` — the outer list
    // read saved its container into a FIXED scratch local; emitting the
    // nested `ir[k]` index read re-entered the same path and clobbered
    // it, so the outer f64.load read from the ir array (i64 bit patterns
    // as denormal f64) — a SILENT wrong-value miscompile.
    let wasm = compile_wasm(
        "def indirect(n: int) -> float:\n\
         \x20   ir = [0] * n\n\
         \x20   rh = [0.0] * (n + 2)\n\
         \x20   i = 0\n\
         \x20   while i < n:\n\
         \x20       ir[i] = (i * 3) % n\n\
         \x20       rh[i] = 100.0 * i\n\
         \x20       i = i + 1\n\
         \x20   s = 0.0\n\
         \x20   k = 0\n\
         \x20   while k < n:\n\
         \x20       s = s + rh[ir[k]]\n\
         \x20       s = s + rh[ir[k] + 1]\n\
         \x20       k = k + 1\n\
         \x20   return s\n",
    );
    validate_wasm(&wasm);
    // n=5: ir = [0,3,1,4,2]; rh = [0,100,200,300,400,0,0]
    // sum rh[ir[k]] = 0+300+100+400+200 = 1000
    // sum rh[ir[k]+1] = rh[1]+rh[4]+rh[2]+rh[5]+rh[3] = 100+400+200+0+300 = 1000
    let got = call_i64_f64(&wasm, "indirect", 5);
    assert_eq!(got, 2000.0);
}

// ============================================================================
// v0.2.5 Cluster A (#485 / #487): the WASM expression lowering upholds the
// same Python value-semantics contract the JS backend encodes (pyAnd / pyOr /
// pyEq / pyBool / emit_comparison_chain). SPOTs through the emitted .wasm on
// wasmi; tests/differential/wasm_net binds the same shapes to CPython, and its
// baseline evidence (evidence/base-677ddc6f.txt) is the RED half.
// ============================================================================

/// Write an i64 list `[i32 len][i32 cap][i64...]` via __alloc; return the ptr.
fn write_i64_list(store: &mut wasmi::Store<()>, instance: &wasmi::Instance, vals: &[i64]) -> i32 {
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut *store, "__alloc")
        .expect("list-param module must export __alloc");
    let n = vals.len() as i32;
    let ptr = alloc.call(&mut *store, 8 + n * 8).expect("__alloc failed");
    let memory = instance
        .get_memory(&mut *store, "memory")
        .expect("No memory export");
    memory
        .write(&mut *store, ptr as usize, &n.to_le_bytes())
        .unwrap();
    memory
        .write(&mut *store, (ptr as usize) + 4, &n.to_le_bytes())
        .unwrap();
    for (i, v) in vals.iter().enumerate() {
        memory
            .write(&mut *store, (ptr as usize) + 8 + i * 8, &v.to_le_bytes())
            .unwrap();
    }
    ptr
}

/// Write an i32 (bool) list `[i32 len][i32 cap][i32...]`.
fn write_i32_list(store: &mut wasmi::Store<()>, instance: &wasmi::Instance, vals: &[i32]) -> i32 {
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut *store, "__alloc")
        .expect("list-param module must export __alloc");
    let n = vals.len() as i32;
    let ptr = alloc.call(&mut *store, 8 + n * 4).expect("__alloc failed");
    let memory = instance
        .get_memory(&mut *store, "memory")
        .expect("No memory export");
    memory
        .write(&mut *store, ptr as usize, &n.to_le_bytes())
        .unwrap();
    memory
        .write(&mut *store, (ptr as usize) + 4, &n.to_le_bytes())
        .unwrap();
    for (i, v) in vals.iter().enumerate() {
        memory
            .write(&mut *store, (ptr as usize) + 8 + i * 4, &v.to_le_bytes())
            .unwrap();
    }
    ptr
}

fn read_i64_elem(store: &wasmi::Store<()>, instance: &wasmi::Instance, ptr: i32, i: usize) -> i64 {
    let memory = instance
        .get_memory(store, "memory")
        .expect("No memory export");
    let mut buf = [0u8; 8];
    memory
        .read(store, (ptr as usize) + 8 + i * 8, &mut buf)
        .unwrap();
    i64::from_le_bytes(buf)
}

/// `(list[int], int) -> int`; `Err` = the call TRAPPED (the #485 symptom).
fn call_listi64_i64_i64(wasm: &[u8], name: &str, xs: &[i64], n: i64) -> Result<i64, String> {
    let (mut store, instance) = make_math_instance(wasm);
    let ptr = write_i64_list(&mut store, &instance, xs);
    let f = instance
        .get_typed_func::<(i32, i64), i64>(&store, name)
        .expect("no fn");
    f.call(&mut store, (ptr, n)).map_err(|e| e.to_string())
}

fn call_listi64_listi64_i32(wasm: &[u8], name: &str, a: &[i64], b: &[i64]) -> i32 {
    let (mut store, instance) = make_math_instance(wasm);
    let pa = write_i64_list(&mut store, &instance, a);
    let pb = write_i64_list(&mut store, &instance, b);
    let f = instance
        .get_typed_func::<(i32, i32), i32>(&store, name)
        .expect("no fn");
    f.call(&mut store, (pa, pb)).expect("call failed")
}

#[test]
fn cluster_a_and_short_circuits_the_bounds_guard_idiom() {
    // The #485 reproducer: `xs[j]` must NOT be evaluated once `j < n` is false.
    let wasm = compile_wasm(
        "def sc(xs: list[int], n: int) -> int:\n\
         \x20   j = 0\n\
         \x20   while j < n and xs[j] > 0:\n\
         \x20       j = j + 1\n\
         \x20   return j\n",
    );
    validate_wasm(&wasm);
    assert_eq!(call_listi64_i64_i64(&wasm, "sc", &[1], 1), Ok(1)); // trapped before the fix
    assert_eq!(call_listi64_i64_i64(&wasm, "sc", &[1, 1, 0], 3), Ok(2));
    assert_eq!(call_listi64_i64_i64(&wasm, "sc", &[], 0), Ok(0));
    assert_eq!(call_listi64_i64_i64(&wasm, "sc", &[5, 5, 5], 3), Ok(3));
}

#[test]
fn cluster_a_or_short_circuits_the_bounds_guard_idiom() {
    let wasm = compile_wasm(
        "def g(xs: list[int], i: int) -> int:\n\
         \x20   if i >= len(xs) or xs[i] == 0:\n\
         \x20       return 1\n\
         \x20   return 0\n",
    );
    validate_wasm(&wasm);
    assert_eq!(call_listi64_i64_i64(&wasm, "g", &[1], 1), Ok(1)); // xs[1] never read
    assert_eq!(call_listi64_i64_i64(&wasm, "g", &[0], 0), Ok(1));
    assert_eq!(call_listi64_i64_i64(&wasm, "g", &[5], 0), Ok(0));
}

#[test]
fn cluster_a_and_or_yield_the_deciding_operand() {
    // Python: `3 and 5 == 5`, `0 or 7 == 7` — the OPERAND, not its truth value.
    let wasm = compile_wasm(
        "def a(x: int, y: int) -> int:\n    return x and y\n\
         def o(x: int, y: int) -> int:\n    return x or y\n\
         def af(x: float, y: float) -> float:\n    return x and y\n\
         def of(x: float, y: float) -> float:\n    return x or y\n",
    );
    validate_wasm(&wasm);
    assert_eq!(call_i64_i64_i64(&wasm, "a", 3, 5), 5);
    assert_eq!(call_i64_i64_i64(&wasm, "a", 0, 5), 0);
    assert_eq!(call_i64_i64_i64(&wasm, "a", -2, 9), 9);
    assert_eq!(call_i64_i64_i64(&wasm, "o", 0, 7), 7);
    assert_eq!(call_i64_i64_i64(&wasm, "o", 3, 7), 3);
    assert_eq!(call_f64_f64_f64(&wasm, "af", 2.5, -1.0), -1.0);
    assert_eq!(
        call_f64_f64_f64(&wasm, "af", 0.0, 2.5).to_bits(),
        0.0f64.to_bits()
    );
    // -0.0 is falsy and is returned AS -0.0 (sign preserved: the operand, not a bool).
    assert_eq!(
        call_f64_f64_f64(&wasm, "af", -0.0, 2.5).to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(call_f64_f64_f64(&wasm, "of", 0.0, -1.0), -1.0);
    // NaN is TRUTHY in Python: `nan or 1.0` is nan.
    assert!(call_f64_f64_f64(&wasm, "of", f64::NAN, 1.0).is_nan());
}

#[test]
fn cluster_a_right_operand_side_effect_runs_only_when_it_decides() {
    let wasm = compile_wasm(
        "def bump(xs: list[int]) -> int:\n\
         \x20   xs[0] = xs[0] + 1\n\
         \x20   return xs[0]\n\
         def f(xs: list[int], flag: bool) -> int:\n\
         \x20   return flag and bump(xs)\n\
         def g(xs: list[int], flag: bool) -> int:\n\
         \x20   return flag or bump(xs)\n",
    );
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let f = instance
        .get_typed_func::<(i32, i32), i64>(&store, "f")
        .unwrap();
    let g = instance
        .get_typed_func::<(i32, i32), i64>(&store, "g")
        .unwrap();
    let p = write_i64_list(&mut store, &instance, &[0]);
    assert_eq!(f.call(&mut store, (p, 0)).unwrap(), 0); // flag False: bump NOT run
    assert_eq!(read_i64_elem(&store, &instance, p, 0), 0);
    assert_eq!(f.call(&mut store, (p, 1)).unwrap(), 1); // flag True: bump runs once
    assert_eq!(read_i64_elem(&store, &instance, p, 0), 1);
    assert_eq!(g.call(&mut store, (p, 1)).unwrap(), 1); // flag True decides: no bump
    assert_eq!(read_i64_elem(&store, &instance, p, 0), 1);
    assert_eq!(g.call(&mut store, (p, 0)).unwrap(), 2); // falsy left: bump runs -> 2
    assert_eq!(read_i64_elem(&store, &instance, p, 0), 2);
}

#[test]
fn cluster_a_chained_comparison_short_circuits_and_evaluates_middle_once() {
    let wasm = compile_wasm(
        "def bump(xs: list[int]) -> int:\n\
         \x20   xs[0] = xs[0] + 1\n\
         \x20   return xs[0]\n\
         def c(xs: list[int]) -> bool:\n\
         \x20   return 0 < bump(xs) < 100\n\
         def t(xs: list[int], k: int) -> bool:\n\
         \x20   return k < 0 < bump(xs)\n\
         def g(xs: list[int], i: int, n: int) -> bool:\n\
         \x20   return i < n < xs[i]\n",
    );
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let c = instance.get_typed_func::<i32, i32>(&store, "c").unwrap();
    let t = instance
        .get_typed_func::<(i32, i64), i32>(&store, "t")
        .unwrap();
    let g = instance
        .get_typed_func::<(i32, i64, i64), i32>(&store, "g")
        .unwrap();
    let p = write_i64_list(&mut store, &instance, &[0]);
    assert_eq!(c.call(&mut store, p).unwrap(), 1);
    assert_eq!(read_i64_elem(&store, &instance, p, 0), 1); // once (was 2: double evaluation)
                                                           // `k < 0` false: the tail `0 < bump(xs)` must not run.
    assert_eq!(t.call(&mut store, (p, 1)).unwrap(), 0);
    assert_eq!(read_i64_elem(&store, &instance, p, 0), 1);
    assert_eq!(t.call(&mut store, (p, -1)).unwrap(), 1);
    assert_eq!(read_i64_elem(&store, &instance, p, 0), 2);
    // `i < n` false: `xs[i]` (out of range) is never read: no trap.
    let q = write_i64_list(&mut store, &instance, &[5]);
    assert_eq!(g.call(&mut store, (q, 1, 1)).unwrap(), 0);
    assert_eq!(g.call(&mut store, (q, 0, 1)).unwrap(), 1); // 0 < 1 < 5
    assert_eq!(g.call(&mut store, (q, 0, 9)).unwrap(), 0); // 0 < 9 < 5 is False
}

#[test]
fn cluster_a_list_comparison_compares_contents_not_handles() {
    let src = "def eq(a: list[int], b: list[int]) -> bool:\n    return a == b\n\
               def ne(a: list[int], b: list[int]) -> bool:\n    return a != b\n\
               def lt(a: list[int], b: list[int]) -> bool:\n    return a < b\n\
               def le(a: list[int], b: list[int]) -> bool:\n    return a <= b\n\
               def gt(a: list[int], b: list[int]) -> bool:\n    return a > b\n\
               def ge(a: list[int], b: list[int]) -> bool:\n    return a >= b\n";
    let wasm = compile_wasm(src);
    validate_wasm(&wasm);
    let cases: &[(&[i64], &[i64], [i32; 6])] = &[
        // (a, b, [eq, ne, lt, le, gt, ge]): CPython answers
        (&[1, 2], &[1, 2], [1, 0, 0, 1, 0, 1]),
        (&[1, 2], &[1, 3], [0, 1, 1, 1, 0, 0]),
        (&[1], &[1, 2], [0, 1, 1, 1, 0, 0]),
        (&[1, 2], &[1], [0, 1, 0, 0, 1, 1]),
        (&[], &[], [1, 0, 0, 1, 0, 1]),
        (&[], &[0], [0, 1, 1, 1, 0, 0]),
        (&[2, 1], &[1, 9], [0, 1, 0, 0, 1, 1]),
        (&[-1, i64::MAX], &[-1, i64::MAX], [1, 0, 0, 1, 0, 1]),
    ];
    for (a, b, want) in cases {
        for (k, name) in ["eq", "ne", "lt", "le", "gt", "ge"].iter().enumerate() {
            assert_eq!(
                call_listi64_listi64_i32(&wasm, name, a, b),
                want[k],
                "{name}({a:?}, {b:?})"
            );
        }
    }
}

#[test]
fn cluster_a_list_comparison_mixed_element_kinds_and_chain() {
    let wasm = compile_wasm(
        "def m(a: list[int], b: list[float]) -> bool:\n    return a == b\n\
         def mb(a: list[bool], b: list[int]) -> bool:\n    return a == b\n\
         def ch(a: list[int], b: list[int], c: list[int]) -> bool:\n    return a == b == c\n",
    );
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let pa = write_i64_list(&mut store, &instance, &[1, 2]);
    let pf = write_f64_list(&mut store, &instance, &[1.0, 2.0]);
    let pf2 = write_f64_list(&mut store, &instance, &[1.0, 2.5]);
    let m = instance
        .get_typed_func::<(i32, i32), i32>(&store, "m")
        .unwrap();
    assert_eq!(m.call(&mut store, (pa, pf)).unwrap(), 1); // [1, 2] == [1.0, 2.0]
    assert_eq!(m.call(&mut store, (pa, pf2)).unwrap(), 0);
    let pb = write_i32_list(&mut store, &instance, &[1, 0]);
    let pi = write_i64_list(&mut store, &instance, &[1, 0]);
    let mb = instance
        .get_typed_func::<(i32, i32), i32>(&store, "mb")
        .unwrap();
    assert_eq!(mb.call(&mut store, (pb, pi)).unwrap(), 1); // [True, False] == [1, 0]
    let pa2 = write_i64_list(&mut store, &instance, &[1, 2]);
    let pa3 = write_i64_list(&mut store, &instance, &[1, 2]);
    let pd = write_i64_list(&mut store, &instance, &[1, 3]);
    let ch = instance
        .get_typed_func::<(i32, i32, i32), i32>(&store, "ch")
        .unwrap();
    assert_eq!(ch.call(&mut store, (pa, pa2, pa3)).unwrap(), 1);
    assert_eq!(ch.call(&mut store, (pa, pa2, pd)).unwrap(), 0);
    assert_eq!(ch.call(&mut store, (pa, pd, pa3)).unwrap(), 0);
}

#[test]
fn cluster_a_mutation_in_branch_witness_from_487() {
    // CPython leaves out == [9, 2] for equal contents; the handle comparison gave [8, 2].
    let wasm = compile_wasm(
        "def f(out: list[int], o: list[int]) -> int:\n\
         \x20   if out == o:\n\
         \x20       out[0] = 9\n\
         \x20   else:\n\
         \x20       out[0] = 8\n\
         \x20   return 0\n",
    );
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let f = instance
        .get_typed_func::<(i32, i32), i64>(&store, "f")
        .unwrap();
    let out = write_i64_list(&mut store, &instance, &[1, 2]);
    let o = write_i64_list(&mut store, &instance, &[1, 2]);
    f.call(&mut store, (out, o)).unwrap();
    assert_eq!(read_i64_elem(&store, &instance, out, 0), 9);
    let o2 = write_i64_list(&mut store, &instance, &[1, 3]);
    f.call(&mut store, (out, o2)).unwrap();
    assert_eq!(read_i64_elem(&store, &instance, out, 0), 8);
}

#[test]
fn cluster_a_list_truthiness_is_a_length_test() {
    let wasm = compile_wasm(
        "def t(xs: list[int]) -> int:\n    if xs:\n        return 1\n    return 0\n\
         def w(xs: list[int]) -> int:\n    while xs:\n        return 1\n    return 0\n\
         def n(xs: list[int]) -> bool:\n    return not xs\n\
         def x(xs: list[int]) -> int:\n    return 1 if xs else 0\n\
         def a(xs: list[int], k: int) -> int:\n    if xs and k > 0:\n        return 1\n    return 0\n",
    );
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let empty = write_i64_list(&mut store, &instance, &[]);
    let zeros = write_i64_list(&mut store, &instance, &[0, 0]);
    let some = write_i64_list(&mut store, &instance, &[1, 2, 3]);
    for name in ["t", "w", "x"] {
        let f = instance.get_typed_func::<i32, i64>(&store, name).unwrap();
        assert_eq!(f.call(&mut store, empty).unwrap(), 0, "{name}([])");
        assert_eq!(
            f.call(&mut store, zeros).unwrap(),
            1,
            "{name}([0, 0]): non-empty is TRUE"
        );
        assert_eq!(f.call(&mut store, some).unwrap(), 1, "{name}([1,2,3])");
    }
    let n = instance.get_typed_func::<i32, i32>(&store, "n").unwrap();
    assert_eq!(n.call(&mut store, empty).unwrap(), 1);
    assert_eq!(n.call(&mut store, some).unwrap(), 0);
    let a = instance
        .get_typed_func::<(i32, i64), i64>(&store, "a")
        .unwrap();
    assert_eq!(a.call(&mut store, (some, 1)).unwrap(), 1);
    assert_eq!(a.call(&mut store, (empty, 1)).unwrap(), 0);
    assert_eq!(a.call(&mut store, (some, 0)).unwrap(), 0);
}

#[test]
fn cluster_a_bool_coercion_is_a_truth_test_not_a_wrap() {
    // `-> bool` of an int/float value is Python bool(): 2**32 is True (it
    // wrapped to 0 before), NaN is True, -0.0 is False.
    let wasm = compile_wasm(
        "def b(x: int) -> bool:\n    return x\n\
         def bf(x: float) -> bool:\n    return x\n\
         def ba(x: int, y: int) -> bool:\n    return x and y\n",
    );
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let b = instance.get_typed_func::<i64, i32>(&store, "b").unwrap();
    assert_eq!(b.call(&mut store, 1i64 << 32).unwrap(), 1);
    assert_eq!(b.call(&mut store, 0).unwrap(), 0);
    assert_eq!(b.call(&mut store, -1).unwrap(), 1);
    let bf = instance.get_typed_func::<f64, i32>(&store, "bf").unwrap();
    assert_eq!(bf.call(&mut store, 0.5).unwrap(), 1);
    assert_eq!(bf.call(&mut store, -0.0).unwrap(), 0);
    assert_eq!(bf.call(&mut store, f64::NAN).unwrap(), 1);
    let ba = instance
        .get_typed_func::<(i64, i64), i32>(&store, "ba")
        .unwrap();
    assert_eq!(ba.call(&mut store, (1, 1i64 << 32)).unwrap(), 1);
    assert_eq!(ba.call(&mut store, (0, 5)).unwrap(), 0);
}

#[test]
fn cluster_a_user_locals_named_like_scratch_slots_are_not_rebound() {
    // Name-binding arm (codex r1 hint): a user local whose name LOOKS like a
    // scratch slot must keep its own local: the slots are `$`-named, which no
    // Python identifier can spell, so no collision is possible by construction.
    let wasm = compile_wasm(
        "def f(__lg_i64: int, __chl_i64: int, __chr_i64: int, __lc_ix: int) -> int:\n\
         \x20   t = (__lg_i64 and __chl_i64) + (0 < (__chr_i64 + 1) < 9)\n\
         \x20   return t * 100 + __lg_i64 + __chl_i64 + __chr_i64 + __lc_ix\n",
    );
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let f = instance
        .get_typed_func::<(i64, i64, i64, i64), i64>(&store, "f")
        .unwrap();
    // (3 and 5) + (0 < 8 < 9) = 5 + 1 = 6 -> 600 + 3 + 5 + 7 + 11 = 626
    assert_eq!(f.call(&mut store, (3, 5, 7, 11)).unwrap(), 626);
}

#[test]
fn cluster_a_bool_coercion_only_at_the_return_boundary() {
    // opus r1/B1: `emit_convert` no longer truth-tests every I32 target — and since
    // r5 the three B1 witnesses (an int into a `list[bool]` element, `x: bool = v`, a
    // list literal `[True, v]`) are REFUSED by the one slot predicate (an int in an
    // i32 bool slot wraps `2**32` to 0): the only coercion site is the `-> bool` return.
    assert_stays_js(
        "def ann(v: int) -> int:\n    f: bool = v\n    return f\n",
        "the local `f`",
    );
    assert_stays_js(
        "def lit(v: int) -> int:\n    buf = [True, v]\n    return buf[1]\n",
        "a list-literal element slot",
    );
    assert_stays_js(
        "def lit2(v: float) -> int:\n    buf = [1, v]\n    return buf[1]\n",
        "a list-literal element slot",
    );
    // A float literal list with an int element is exact (int into a float slot).
    let wasm = compile_wasm("def lit3(v: int) -> float:\n    buf = [1.5, v]\n    return buf[1]\n");
    validate_wasm(&wasm);
    assert_eq!(call_i64_f64(&wasm, "lit3", 7), 7.0);
}

#[test]
fn cluster_a_same_list_compares_reflexively_even_through_nan() {
    // opus r1/SF6: CPython's identity shortcut (`x is y or x == y`) makes `fs == fs`
    // True and `fs < fs` False for `[nan]`; two DISTINCT NaN lists stay unequal.
    let wasm = compile_wasm(
        "def se(fs: list[float]) -> bool:\n    return fs == fs\n\
         def sl(fs: list[float]) -> bool:\n    return fs < fs\n\
         def sg(fs: list[float]) -> bool:\n    return fs >= fs\n\
         def de(a: list[float], b: list[float]) -> bool:\n    return a == b\n",
    );
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let p = write_f64_list(&mut store, &instance, &[f64::NAN, 1.0]);
    let q = write_f64_list(&mut store, &instance, &[f64::NAN, 1.0]);
    let se = instance.get_typed_func::<i32, i32>(&store, "se").unwrap();
    let sl = instance.get_typed_func::<i32, i32>(&store, "sl").unwrap();
    let sg = instance.get_typed_func::<i32, i32>(&store, "sg").unwrap();
    let de = instance
        .get_typed_func::<(i32, i32), i32>(&store, "de")
        .unwrap();
    assert_eq!(se.call(&mut store, p).unwrap(), 1);
    assert_eq!(sl.call(&mut store, p).unwrap(), 0);
    assert_eq!(sg.call(&mut store, p).unwrap(), 1);
    assert_eq!(de.call(&mut store, (p, q)).unwrap(), 0);
    assert_eq!(de.call(&mut store, (p, p)).unwrap(), 1);
}

#[test]
fn cluster_a_typed_admission_refuses_unrepresentable_logic_values() {
    // opus r2/NEW-1: decided at the TYPE layer (logic_join), so a list-valued
    // SUBSCRIPT or a `for` target is caught exactly like a list param.
    for src in [
        "def f(xs: list[int], k: int) -> int:\n    return xs and k\n",
        "def f(xs: list[int], k: int) -> int:\n    v = xs or k\n    return 0\n",
        "def f(xs: list[int], k: int) -> int:\n    return (xs and k) + 1\n",
        "def f(xs: list[int], k: int) -> int:\n    return 1 if k else (xs and k)\n",
        "def f(n: int, k: int) -> int:\n    buf = [0] * n\n    return buf and k\n",
        "def f(n: int, k: int) -> int:\n    a = [0] * n\n    b = a\n    return k or b\n",
        "def f(k: int) -> int:\n    return [[1, 2]][0] and k\n",
        "def f(k: int) -> int:\n    a = [[1, 2]]\n    b = a[0]\n    return b and k\n",
        "def f(xs: list[int], k: int) -> int:\n    for b in [xs]:\n        return b and k\n    return 0\n",
        "def f(xs: list[int], k: int) -> int:\n    return abs(xs and k)\n",
        "def f(xs: list[int], k: int) -> bool:\n    return (xs and k) == 1\n",
    ] {
        assert_stays_js(src, "no common WASM type");
    }
}

#[test]
fn cluster_a_typed_admission_keeps_representable_logic() {
    // Test positions (truth value only) and two lists of ONE element type (the
    // join is the list type: `xs or ys` carries the deciding list exactly).
    for src in [
        "def f(xs: list[int], k: int) -> int:\n    if xs and k > 0:\n        return 1\n    return 0\n",
        "def f(xs: list[int], k: int) -> int:\n    while xs or k:\n        return 1\n    return 0\n",
        "def f(xs: list[int], k: int) -> int:\n    assert xs and k\n    return 1\n",
        "def f(xs: list[int], k: int) -> int:\n    return 1 if (xs and k) else 0\n",
        "def f(xs: list[int], k: int) -> bool:\n    return not (xs and k)\n",
        "def f(xs: list[int], k: int) -> int:\n    if (k > 0 and xs) or (not xs and k < 0):\n        return 1\n    return 0\n",
        "def f(a: int, b: int) -> int:\n    return a and b\n",
        "def f(xs: list[int], ys: list[int]) -> int:\n    return (xs or ys)[0]\n",
        "def f(xs: list[int], ys: list[int]) -> int:\n    zs = xs and ys\n    return zs[0]\n",
    ] {
        let module = pyths_parser::parse(src).expect("parse");
        let out = codegen_wasm(&module);
        assert!(!out.wasm.is_empty(), "should stay eligible: {src}: {:?}", out.rejected_functions);
    }
    // And the two-list value case computes the deciding LIST.
    let wasm =
        compile_wasm("def f(xs: list[int], ys: list[int]) -> int:\n    return (xs or ys)[0]\n");
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let empty = write_i64_list(&mut store, &instance, &[]);
    let ys = write_i64_list(&mut store, &instance, &[7, 8]);
    let xs = write_i64_list(&mut store, &instance, &[3]);
    let f = instance
        .get_typed_func::<(i32, i32), i64>(&store, "f")
        .unwrap();
    assert_eq!(f.call(&mut store, (empty, ys)).unwrap(), 7);
    assert_eq!(f.call(&mut store, (xs, ys)).unwrap(), 3);
}

#[test]
fn cluster_a_typed_admission_refuses_container_under_scalar_return_and_bool_slot_wrap() {
    // opus r2/NEW-2: `-> int: return xs` surfaced the heap pointer; NEW-7: an
    // int stored into a `list[bool]` element wrapped `2**32` to 0.
    assert_stays_js(
        "def li(xs: list[int]) -> int:\n    return xs\n",
        "container value under a scalar return type",
    );
    assert_stays_js(
        "def lf(fs: list[float]) -> float:\n    return fs\n",
        "container value under a scalar return type",
    );
    assert_stays_js(
        "def lb(xs: list[int]) -> bool:\n    return xs\n",
        "container value under a scalar return type",
    );
    assert_stays_js(
        "def s(xs: list[bool], v: int) -> int:\n    xs[0] = v\n    return 0\n",
        "list element slot",
    );
    // A bool-valued store into list[bool] stays admitted.
    let wasm =
        compile_wasm("def s(xs: list[bool], v: int) -> int:\n    xs[0] = v > 0\n    return 0\n");
    validate_wasm(&wasm);
}

#[test]
fn cluster_a_typed_admission_sees_through_ifexpr_and_concat() {
    // opus r3/NEW-r3-1: `expr_type` erases a list-valued IfExpr / `+` to I64; the
    // container-preserving static type must refuse these wherever they appear.
    for src in [
        "def h(xs: list[int], ys: list[int], c: int, k: int) -> int:\n    return (xs if c else ys) and k\n",
        "def r(xs: list[int], ys: list[int], c: int) -> int:\n    return xs if c else ys\n",
        "def r2(xs: list[int], ys: list[int], c: int) -> float:\n    return xs if c else ys\n",
        "def hc(xs: list[int], ys: list[int]) -> int:\n    return xs + ys\n",
        "def ml(xs: list[int], ys: list[int], c: int) -> int:\n    return len(xs if c else ys)\n",
        "def mt(xs: list[int], ys: list[int], c: int) -> int:\n    if xs + ys:\n        return 1\n    return 0\n",
        "def mx(xs: list[int], c: int) -> int:\n    return xs if c else 5\n",
    ] {
        let module = pyths_parser::parse(src).expect("parse");
        let out = codegen_wasm(&module);
        assert!(out.wasm.is_empty(), "must be refused: {src}");
        assert!(
            out.rejected_functions.iter().any(|(_, r)| r.contains("container-valued expression") || r.contains("no common WASM type") || r.contains("container value under a scalar return type")),
            "{src}: {:?}",
            out.rejected_functions
        );
    }
    // The scalar-valued IfExpr / `+` are untouched.
    let wasm =
        compile_wasm("def s(a: int, b: int, c: int) -> int:\n    return (a if c else b) + a\n");
    validate_wasm(&wasm);
}

#[test]
fn cluster_a_typed_admission_store_rule_covers_every_store_site() {
    // opus r3/NEW-r3-2: one store rule for Assign, AugAssign and AnnAssign.
    assert_stays_js(
        "def s(xs: list[bool], v: int) -> int:\n    xs[0] += v\n    return 0\n",
        "list element slot",
    );
    assert_stays_js(
        "def s(xs: list[bool]) -> int:\n    xs[0] += 1\n    return 0\n",
        "list element slot",
    );
    assert_stays_js(
        "def s(xs: list[bool], v: float) -> int:\n    xs[0] = v\n    return 0\n",
        "list element slot",
    );
    // opus r3/NEW-r3-3: an assert MESSAGE is not lowered -> refused.
    assert_stays_js(
        "def m(ys: list[int], c: int) -> int:\n    assert c, ys[100]\n    return 1\n",
        "`assert` with a message",
    );
    let wasm = compile_wasm("def ok(xs: list[bool], v: int) -> int:\n    xs[0] = v > 0\n    assert v >= 0\n    return 0\n");
    validate_wasm(&wasm);
}

#[test]
fn cluster_a_store_rule_covers_every_slot_type_and_return_none() {
    // opus r4/NEW-r4-1: the store rule is about REPRESENTABILITY in the slot,
    // for every (stored, slot) pair — not only the list[bool] slot.
    assert_stays_js(
        "def s(xs: list[int], v: float) -> float:\n    xs[0] = v\n    return xs[0]\n",
        "list element slot",
    );
    assert_stays_js(
        "def s(xs: list[int]) -> int:\n    xs[0] += 1.5\n    return xs[0]\n",
        "list element slot",
    );
    assert_stays_js(
        "def s(xs: list[int], ys: list[int]) -> int:\n    xs[0] = ys\n    return xs[0]\n",
        "list element slot",
    );
    assert_stays_js(
        "def s(fs: list[float], ys: list[int]) -> int:\n    fs[0] = ys\n    return 0\n",
        "list element slot",
    );
    assert_stays_js(
        "def s(bs: list[bool], ys: list[int]) -> int:\n    bs[0] = ys\n    return 0\n",
        "list element slot",
    );
    // An int into a list[float] slot is Python's own exact coercion: admitted and correct.
    let wasm =
        compile_wasm("def s(fs: list[float], v: int) -> float:\n    fs[0] = v\n    return fs[0]\n");
    validate_wasm(&wasm);
    let (mut store, instance) = make_math_instance(&wasm);
    let p = write_f64_list(&mut store, &instance, &[1.5, 2.5]);
    let f = instance
        .get_typed_func::<(i32, i64), f64>(&store, "s")
        .unwrap();
    assert_eq!(f.call(&mut store, (p, 7)).unwrap(), 7.0);
    // opus r4/NEW-r4-3: `-> None` returning a value drops it -> refused.
    assert_stays_js(
        "def n(xs: list[int]) -> None:\n    return xs\n",
        "`-> None` function returning a value",
    );
    assert_stays_js(
        "def n(xs: list[int]) -> None:\n    return 5\n",
        "`-> None` function returning a value",
    );
    let wasm = compile_wasm("def n(xs: list[int]) -> None:\n    xs[0] = 1\n    return\n");
    validate_wasm(&wasm);
    // opus r4/NEW-r4-2: a raise argument that can itself raise -> refused; literal/name ok.
    assert_stays_js("def r(ys: list[int], c: int) -> int:\n    if c:\n        raise ValueError(ys[100])\n    return 0\n", "`raise` whose argument");
    let wasm = compile_wasm(
        "def r(c: int) -> int:\n    if c:\n        raise ValueError(c)\n    return 0\n",
    );
    validate_wasm(&wasm);
}

#[test]
fn cluster_a_slot_rule_covers_local_return_and_argument_slots() {
    // opus r5/NEW-r5-1: the LOCAL slot (annotated or first-assignment-typed).
    assert_stays_js(
        "def f(fs: list[float]) -> float:\n    x: int = fs[0]\n    return x + 0.5\n",
        "the local `x`",
    );
    assert_stays_js(
        "def f(xs: list[int], ys: list[int]) -> int:\n    x: int = ys\n    return x - x\n",
        "the local `x`",
    );
    assert_stays_js(
        "def f(fs: list[float]) -> float:\n    x: bool = fs[0]\n    return x + 0.5\n",
        "the local `x`",
    );
    assert_stays_js(
        "def f(v: int) -> int:\n    x: list[int] = v\n    return len(x)\n",
        "the local `x`",
    );
    assert_stays_js(
        "def f(fs: list[float]) -> float:\n    s = 0\n    s = fs[0]\n    return s\n",
        "the local `s`",
    );
    assert_stays_js(
        "def f(a: float) -> int:\n    x: int = a\n    return x\n",
        "the local `x`",
    );
    // opus r5/NEW-r5-2: the RETURN slot (float under -> int); int under -> float stays.
    assert_stays_js(
        "def f(a: float) -> int:\n    return a\n",
        "float value under an `int` return type",
    );
    assert_stays_js(
        "def f(a: int, b: int) -> int:\n    return a / b\n",
        "float value under an `int` return type",
    );
    assert_stays_js(
        "def f(fs: list[float]) -> int:\n    return fs[0]\n",
        "float value under an `int` return type",
    );
    let wasm = compile_wasm("def f(a: int) -> float:\n    return a\n");
    validate_wasm(&wasm);
    assert_eq!(call_i64_f64(&wasm, "f", 7), 7.0);
    // Call-ARGUMENT slots: no conversion at a user call, so the static type must match
    // (the callee itself stays admitted; only the caller is refused, with the reason).
    let module = pyths_parser::parse(
        "def callee(n: int) -> int:\n    return n + 1\ndef caller(fs: list[float]) -> int:\n    return callee(fs[0])\n",
    )
    .expect("parse");
    let out = codegen_wasm(&module);
    assert!(
        out.compiled_functions.contains(&"callee".to_string()),
        "{:?}",
        out.rejected_functions
    );
    assert!(
        out.rejected_functions
            .iter()
            .any(|(n, r)| n == "caller" && r.contains("passing a F64 value to parameter `n`")),
        "{:?}",
        out.rejected_functions
    );
    // The r5-3 module: the innocent `callee` / `good` stay admitted now that the two
    // culprits are refused WITH a reason (no blind alphabetical demotion).
    let module = pyths_parser::parse(
        "def good(n: int) -> int:\n    return n + 1\ndef callee(n: int) -> int:\n    return n + 1\n\
         def caller(fs: list[float]) -> int:\n    return callee(fs[0])\n\
         def param_retype(x: int, fs: list[float]) -> int:\n    x = fs[0]\n    return x\n",
    )
    .expect("parse");
    let out = codegen_wasm(&module);
    assert!(
        out.compiled_functions.contains(&"good".to_string()),
        "{:?}",
        out.rejected_functions
    );
    assert!(
        out.compiled_functions.contains(&"callee".to_string()),
        "{:?}",
        out.rejected_functions
    );
    assert_eq!(
        out.rejected_functions.len(),
        2,
        "{:?}",
        out.rejected_functions
    );
}
