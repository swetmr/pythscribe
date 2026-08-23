use pyths_syntax::ast::*;

/// TypeScript type representation for .d.ts generation.
#[derive(Debug, Clone, PartialEq)]
pub enum TsType {
    Number,
    String,
    Boolean,
    Null,
    Any,
    Void,
    Array(Box<TsType>),
    Tuple(Vec<TsType>),
    Record(Box<TsType>, Box<TsType>),
    Union(Vec<TsType>),
    Nullable(Box<TsType>),
    Set(Box<TsType>),
    Named(std::string::String),
    Callable(Vec<TsType>, Box<TsType>),
}

/// Resolve a Python type annotation Expr to a TsType.
pub fn resolve_ts_type(annotation: &Expr) -> TsType {
    match &annotation.kind {
        ExprKind::Name(n) => match n.as_str() {
            "int" | "float" => TsType::Number,
            "str" => TsType::String,
            "bool" => TsType::Boolean,
            "None" => TsType::Null,
            "Any" => TsType::Any,
            other => {
                if other.chars().next().is_some_and(|c| c.is_uppercase()) {
                    TsType::Named(other.to_string())
                } else {
                    TsType::Any
                }
            }
        },
        ExprKind::NoneLiteral => TsType::Null,
        ExprKind::Subscript { value, index, .. } => {
            if let ExprKind::Name(n) = &value.kind {
                match n.as_str() {
                    "List" | "list" => {
                        let inner = resolve_ts_type(index);
                        TsType::Array(Box::new(inner))
                    }
                    "Dict" | "dict" => {
                        if let ExprKind::Tuple(elts) = &index.kind {
                            let k = elts.first().map(resolve_ts_type).unwrap_or(TsType::Any);
                            let v = elts.get(1).map(resolve_ts_type).unwrap_or(TsType::Any);
                            TsType::Record(Box::new(k), Box::new(v))
                        } else {
                            TsType::Record(Box::new(resolve_ts_type(index)), Box::new(TsType::Any))
                        }
                    }
                    "Optional" => {
                        let inner = resolve_ts_type(index);
                        TsType::Nullable(Box::new(inner))
                    }
                    "Set" | "set" => {
                        let inner = resolve_ts_type(index);
                        TsType::Set(Box::new(inner))
                    }
                    "Tuple" | "tuple" => {
                        if let ExprKind::Tuple(elts) = &index.kind {
                            let types = elts.iter().map(resolve_ts_type).collect();
                            TsType::Tuple(types)
                        } else {
                            TsType::Tuple(vec![resolve_ts_type(index)])
                        }
                    }
                    "Union" => {
                        if let ExprKind::Tuple(elts) = &index.kind {
                            let types = elts.iter().map(resolve_ts_type).collect();
                            TsType::Union(types)
                        } else {
                            resolve_ts_type(index)
                        }
                    }
                    "Callable" => {
                        if let ExprKind::Tuple(elts) = &index.kind {
                            if elts.len() == 2 {
                                let ret = resolve_ts_type(&elts[1]);
                                if let ExprKind::List(params) = &elts[0].kind {
                                    let param_types = params.iter().map(resolve_ts_type).collect();
                                    TsType::Callable(param_types, Box::new(ret))
                                } else {
                                    TsType::Callable(vec![], Box::new(ret))
                                }
                            } else {
                                TsType::Any
                            }
                        } else {
                            TsType::Any
                        }
                    }
                    _ => TsType::Any,
                }
            } else {
                TsType::Any
            }
        }
        _ => TsType::Any,
    }
}

/// Render a TsType to its TypeScript syntax string.
pub fn ts_type_to_string(ty: &TsType) -> std::string::String {
    match ty {
        TsType::Number => "number".to_string(),
        TsType::String => "string".to_string(),
        TsType::Boolean => "boolean".to_string(),
        TsType::Null => "null".to_string(),
        TsType::Any => "any".to_string(),
        TsType::Void => "void".to_string(),
        TsType::Array(inner) => {
            let inner_str = ts_type_to_string(inner);
            // Wrap union/nullable types in parens for array syntax
            match inner.as_ref() {
                TsType::Union(_) | TsType::Nullable(_) => format!("({})[]", inner_str),
                _ => format!("{}[]", inner_str),
            }
        }
        TsType::Tuple(types) => {
            let parts: Vec<_> = types.iter().map(ts_type_to_string).collect();
            format!("[{}]", parts.join(", "))
        }
        TsType::Record(k, v) => {
            format!("Record<{}, {}>", ts_type_to_string(k), ts_type_to_string(v))
        }
        TsType::Union(types) => {
            let parts: Vec<_> = types.iter().map(ts_type_to_string).collect();
            parts.join(" | ")
        }
        TsType::Nullable(inner) => {
            format!("{} | null", ts_type_to_string(inner))
        }
        TsType::Set(inner) => {
            format!("Set<{}>", ts_type_to_string(inner))
        }
        TsType::Named(name) => name.clone(),
        TsType::Callable(params, ret) => {
            let param_parts: Vec<_> = params
                .iter()
                .enumerate()
                .map(|(i, t)| format!("arg{}: {}", i, ts_type_to_string(t)))
                .collect();
            format!("({}) => {}", param_parts.join(", "), ts_type_to_string(ret))
        }
    }
}

/// Generator for TypeScript .d.ts declaration files.
pub struct DtsGenerator {
    output: std::string::String,
    indent: usize,
}

impl Default for DtsGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl DtsGenerator {
    pub fn new() -> Self {
        Self {
            output: std::string::String::new(),
            indent: 0,
        }
    }

    pub fn generate(&mut self, module: &Module) -> std::string::String {
        for stmt in &module.body {
            self.emit_declaration(stmt);
        }
        self.output.clone()
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
    }

    fn emit_declaration(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::FuncDef {
                name,
                params,
                return_type,
                is_async,
                decorator_list,
                ..
            } => {
                // Skip if it's a @component decorator (React component)
                let is_component = decorator_list
                    .iter()
                    .any(|d| matches!(&d.kind, ExprKind::Name(n) if n == "component"));
                if is_component {
                    // Emit as function returning JSX.Element. The declaration
                    // MUST match the JS calling convention: React calls a
                    // `@component` with a SINGLE props object and the JS emitter
                    // destructures it (`function ZipList({names, nums} = {})`),
                    // so the `.d.ts` takes one props object — NOT positional
                    // params (WB-19; TS2786 for any component with >=2 params,
                    // and prop-name-unsound even at arity 1).
                    self.write_indent();
                    self.output.push_str("export declare function ");
                    self.output.push_str(name);
                    self.output.push('(');
                    self.emit_component_params(params);
                    self.output.push_str("): JSX.Element;\n");
                } else {
                    self.emit_func_decl(name, params, return_type.as_ref(), *is_async, true);
                }
            }
            StmtKind::ClassDef {
                name,
                bases,
                body,
                decorator_list,
            } => {
                self.emit_class_decl(name, bases, body, decorator_list);
            }
            StmtKind::AnnAssign {
                target, annotation, ..
            } => {
                if let ExprKind::Name(name) = &target.kind {
                    self.emit_var_decl(name, annotation);
                }
            }
            // Skip imports, control flow, bare expressions
            _ => {}
        }
    }

    fn emit_params(&mut self, params: &[Param]) {
        let mut first = true;
        for param in params {
            if param.name == "self" || param.name == "cls" {
                continue;
            }
            if !first {
                self.output.push_str(", ");
            }
            first = false;

            if param.is_args {
                self.output.push_str("...");
                self.output.push_str(&param.name);
                self.output.push_str(": any[]");
            } else if param.is_kwargs {
                self.output.push_str(&param.name);
                self.output.push_str(": Record<string, any>");
            } else {
                self.output.push_str(&param.name);
                if param.default.is_some() {
                    self.output.push('?');
                }
                self.output.push_str(": ");
                if let Some(ann) = &param.annotation {
                    let ts = resolve_ts_type(ann);
                    self.output.push_str(&ts_type_to_string(&ts));
                } else {
                    self.output.push_str("any");
                }
            }
        }
    }

    /// Emit the parameter list of a `@component` declaration as the SINGLE
    /// props object that the JS emitter actually produces, so the `.d.ts`
    /// signature is callable exactly the way React (and the emitted JS) calls
    /// the component: `Component({ prop, ... })`. Mirrors
    /// `emit.rs::emit_func_def`'s `@component` props convention (WB-19).
    ///
    /// Scope note: this is ONLY for `@component`. `@psx` helpers keep positional
    /// params in the JS (the props-destructure in `emit.rs` is gated on
    /// `is_component`, not `is_psx`), so their positional declaration — emitted
    /// via the ordinary `emit_func_decl` path — already matches and is left
    /// unchanged.
    fn emit_component_params(&mut self, params: &[Param]) {
        let effective: Vec<&Param> = params
            .iter()
            .filter(|p| p.name != "self" && p.name != "cls")
            .collect();
        // 0 params → `()` (React calls `Component()`); already JSX-valid.
        if effective.is_empty() {
            return;
        }
        // Whole-props form: a single `**kwargs` param, or a single no-default
        // param literally named `props`, binds the WHOLE props object in the JS
        // (`function C(props)` / `function C(kwargs = {})`) — one object
        // argument, not a destructure. The existing single-param declaration
        // (`props: any` / `kwargs: Record<string, any>`) is already JSX-callable,
        // so keep it.
        let whole_props_object = effective.len() == 1
            && !effective[0].is_args
            && effective[0].default.is_none()
            && (effective[0].is_kwargs || effective[0].name == "props");
        if whole_props_object {
            self.emit_params(params);
            return;
        }
        // Otherwise the JS destructures the props object
        // (`function C({a, b, ...rest} = {})`). Declare ONE props object whose
        // members are the named params (each `name: any` — pyths is dynamically
        // typed; a defaulted prop becomes optional `name?`), and a trailing
        // `**rest` becomes an open index signature.
        self.output.push_str("props: { ");
        let mut first = true;
        let mut has_rest = false;
        for param in &effective {
            if param.is_args {
                // `*args` is not a member of the destructured props object
                // (the JS emitter drops it from the pattern too).
                continue;
            }
            if param.is_kwargs {
                has_rest = true;
                continue;
            }
            if !first {
                self.output.push_str("; ");
            }
            first = false;
            self.output.push_str(&param.name);
            if param.default.is_some() {
                self.output.push('?');
            }
            self.output.push_str(": any");
        }
        if has_rest {
            if !first {
                self.output.push_str("; ");
            }
            self.output.push_str("[key: string]: any");
        }
        self.output.push_str(" }");
    }

    fn resolve_return_type(return_type: Option<&Expr>) -> TsType {
        if let Some(ret) = return_type {
            let ty = resolve_ts_type(ret);
            // Python `-> None` means void in TypeScript
            if ty == TsType::Null {
                TsType::Void
            } else {
                ty
            }
        } else {
            // No annotation: the function may still return a value, so `any`
            // (usable by consumers) is the safe declaration. Emitting `void`
            // here would falsely claim it returns nothing and break callers
            // that use the result (e.g. `.map()` on a returned list).
            TsType::Any
        }
    }

    fn emit_func_decl(
        &mut self,
        name: &str,
        params: &[Param],
        return_type: Option<&Expr>,
        is_async: bool,
        is_export: bool,
    ) {
        self.write_indent();
        if is_export {
            self.output.push_str("export declare function ");
        } else {
            // Method inside class — no export/declare
        }
        self.output.push_str(name);
        self.output.push('(');
        self.emit_params(params);
        self.output.push_str("): ");

        let ret_type = Self::resolve_return_type(return_type);

        if is_async {
            self.output
                .push_str(&format!("Promise<{}>", ts_type_to_string(&ret_type)));
        } else {
            self.output.push_str(&ts_type_to_string(&ret_type));
        }
        self.output.push_str(";\n");
    }

    fn emit_class_decl(
        &mut self,
        name: &str,
        bases: &[Expr],
        body: &[Stmt],
        decorator_list: &[Expr],
    ) {
        let is_dataclass = decorator_list.iter().any(|d| match &d.kind {
            ExprKind::Name(n) => n == "dataclass",
            ExprKind::Call { func, .. } => {
                matches!(&func.kind, ExprKind::Name(n) if n == "dataclass")
            }
            _ => false,
        });

        self.write_indent();
        self.output.push_str("export declare class ");
        self.output.push_str(name);

        // Emit extends clause
        if !bases.is_empty() {
            self.output.push_str(" extends ");
            let base_names: Vec<_> = bases
                .iter()
                .filter_map(|b| {
                    if let ExprKind::Name(n) = &b.kind {
                        Some(n.clone())
                    } else {
                        None
                    }
                })
                .collect();
            self.output.push_str(&base_names.join(", "));
        }

        self.output.push_str(" {\n");
        self.indent += 1;

        if is_dataclass {
            self.emit_dataclass_body(body);
        } else {
            self.emit_regular_class_body(body);
        }

        self.indent -= 1;
        self.write_indent();
        self.output.push_str("}\n");
    }

    fn emit_dataclass_body(&mut self, body: &[Stmt]) {
        // Dataclass: fields come from AnnAssign statements
        for stmt in body {
            match &stmt.kind {
                StmtKind::AnnAssign {
                    target, annotation, ..
                } => {
                    if let ExprKind::Name(field_name) = &target.kind {
                        self.write_indent();
                        self.output.push_str(field_name);
                        self.output.push_str(": ");
                        let ts = resolve_ts_type(annotation);
                        self.output.push_str(&ts_type_to_string(&ts));
                        self.output.push_str(";\n");
                    }
                }
                StmtKind::FuncDef {
                    name,
                    params,
                    return_type,
                    is_async,
                    ..
                } => {
                    // Skip __init__ for dataclasses (constructor is implied by fields)
                    // But emit other methods
                    if name == "__init__" {
                        continue;
                    }
                    self.emit_method(name, params, return_type.as_ref(), *is_async);
                }
                _ => {}
            }
        }
    }

    fn emit_regular_class_body(&mut self, body: &[Stmt]) {
        // First pass: extract fields from __init__ self.X = ... assignments
        let mut fields_emitted = std::collections::HashSet::new();

        for stmt in body {
            if let StmtKind::FuncDef {
                name,
                params,
                body: func_body,
                ..
            } = &stmt.kind
            {
                if name == "__init__" {
                    // Extract fields from self.X = ... in __init__
                    self.extract_init_fields(func_body, params, &mut fields_emitted);
                }
            }
        }

        // Also emit module-level annotations in the class body
        for stmt in body {
            if let StmtKind::AnnAssign {
                target, annotation, ..
            } = &stmt.kind
            {
                if let ExprKind::Name(field_name) = &target.kind {
                    if !fields_emitted.contains(field_name.as_str()) {
                        self.write_indent();
                        self.output.push_str(field_name);
                        self.output.push_str(": ");
                        let ts = resolve_ts_type(annotation);
                        self.output.push_str(&ts_type_to_string(&ts));
                        self.output.push_str(";\n");
                        fields_emitted.insert(field_name.clone());
                    }
                }
            }
        }

        // Emit methods
        for stmt in body {
            if let StmtKind::FuncDef {
                name,
                params,
                return_type,
                is_async,
                ..
            } = &stmt.kind
            {
                if name == "__init__" {
                    self.emit_constructor(params);
                } else {
                    self.emit_method(name, params, return_type.as_ref(), *is_async);
                }
            }
        }
    }

    fn extract_init_fields(
        &mut self,
        body: &[Stmt],
        params: &[Param],
        fields_emitted: &mut std::collections::HashSet<std::string::String>,
    ) {
        // Build a map from param name → annotation type
        let param_types: std::collections::HashMap<&str, Option<&Expr>> = params
            .iter()
            .map(|p| (p.name.as_str(), p.annotation.as_deref()))
            .collect();

        for stmt in body {
            match &stmt.kind {
                StmtKind::Assign { targets, value } => {
                    for target in targets {
                        if let ExprKind::Attribute {
                            value: obj, attr, ..
                        } = &target.kind
                        {
                            if matches!(&obj.kind, ExprKind::Name(n) if n == "self")
                                && !fields_emitted.contains(attr.as_str())
                            {
                                self.write_indent();
                                self.output.push_str(attr);
                                self.output.push_str(": ");
                                // Try to infer type from the RHS if it's a param name
                                let field_type = if let ExprKind::Name(rhs) = &value.kind {
                                    if let Some(Some(ann)) = param_types.get(rhs.as_str()) {
                                        resolve_ts_type(ann)
                                    } else {
                                        TsType::Any
                                    }
                                } else {
                                    TsType::Any
                                };
                                self.output.push_str(&ts_type_to_string(&field_type));
                                self.output.push_str(";\n");
                                fields_emitted.insert(attr.clone());
                            }
                        }
                    }
                }
                StmtKind::AnnAssign {
                    target, annotation, ..
                } => {
                    if let ExprKind::Attribute {
                        value: obj, attr, ..
                    } = &target.kind
                    {
                        if matches!(&obj.kind, ExprKind::Name(n) if n == "self")
                            && !fields_emitted.contains(attr.as_str())
                        {
                            self.write_indent();
                            self.output.push_str(attr);
                            self.output.push_str(": ");
                            let ts = resolve_ts_type(annotation);
                            self.output.push_str(&ts_type_to_string(&ts));
                            self.output.push_str(";\n");
                            fields_emitted.insert(attr.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn emit_constructor(&mut self, params: &[Param]) {
        self.write_indent();
        self.output.push_str("constructor(");
        self.emit_params(params);
        self.output.push_str(");\n");
    }

    fn emit_method(
        &mut self,
        name: &str,
        params: &[Param],
        return_type: Option<&Expr>,
        is_async: bool,
    ) {
        self.write_indent();
        self.output.push_str(name);
        self.output.push('(');
        self.emit_params(params);
        self.output.push_str("): ");

        let ret_type = Self::resolve_return_type(return_type);

        if is_async {
            self.output
                .push_str(&format!("Promise<{}>", ts_type_to_string(&ret_type)));
        } else {
            self.output.push_str(&ts_type_to_string(&ret_type));
        }
        self.output.push_str(";\n");
    }

    fn emit_var_decl(&mut self, name: &str, annotation: &Expr) {
        self.write_indent();
        self.output.push_str("export declare const ");
        self.output.push_str(name);
        self.output.push_str(": ");
        let ts = resolve_ts_type(annotation);
        self.output.push_str(&ts_type_to_string(&ts));
        self.output.push_str(";\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyths_syntax::span::Span;

    fn dummy_span() -> Span {
        Span::dummy()
    }

    fn name_expr(n: &str) -> Expr {
        Expr::name(n, dummy_span())
    }

    fn subscript_expr(base: &str, index: Expr) -> Expr {
        Expr::new(
            ExprKind::Subscript {
                value: Box::new(name_expr(base)),
                index: Box::new(index),
                optional: false,
            },
            dummy_span(),
        )
    }

    fn tuple_expr(elts: Vec<Expr>) -> Expr {
        Expr::new(ExprKind::Tuple(elts), dummy_span())
    }

    fn list_expr(elts: Vec<Expr>) -> Expr {
        Expr::new(ExprKind::List(elts), dummy_span())
    }

    // --- Sub-task 1: TsType + mapping tests ---

    #[test]
    fn test_ts_type_primitives() {
        assert_eq!(resolve_ts_type(&name_expr("int")), TsType::Number);
        assert_eq!(resolve_ts_type(&name_expr("float")), TsType::Number);
        assert_eq!(resolve_ts_type(&name_expr("str")), TsType::String);
        assert_eq!(resolve_ts_type(&name_expr("bool")), TsType::Boolean);
        assert_eq!(
            resolve_ts_type(&Expr::new(ExprKind::NoneLiteral, dummy_span())),
            TsType::Null
        );
        assert_eq!(resolve_ts_type(&name_expr("None")), TsType::Null);
    }

    #[test]
    fn test_ts_type_list() {
        let ann = subscript_expr("List", name_expr("int"));
        let ts = resolve_ts_type(&ann);
        assert_eq!(ts, TsType::Array(Box::new(TsType::Number)));
        assert_eq!(ts_type_to_string(&ts), "number[]");
    }

    #[test]
    fn test_ts_type_dict() {
        let ann = subscript_expr("Dict", tuple_expr(vec![name_expr("str"), name_expr("int")]));
        let ts = resolve_ts_type(&ann);
        assert_eq!(
            ts,
            TsType::Record(Box::new(TsType::String), Box::new(TsType::Number))
        );
        assert_eq!(ts_type_to_string(&ts), "Record<string, number>");
    }

    #[test]
    fn test_ts_type_optional() {
        let ann = subscript_expr("Optional", name_expr("str"));
        let ts = resolve_ts_type(&ann);
        assert_eq!(ts, TsType::Nullable(Box::new(TsType::String)));
        assert_eq!(ts_type_to_string(&ts), "string | null");
    }

    #[test]
    fn test_ts_type_tuple() {
        let ann = subscript_expr(
            "Tuple",
            tuple_expr(vec![name_expr("int"), name_expr("str")]),
        );
        let ts = resolve_ts_type(&ann);
        assert_eq!(ts, TsType::Tuple(vec![TsType::Number, TsType::String]));
        assert_eq!(ts_type_to_string(&ts), "[number, string]");
    }

    #[test]
    fn test_ts_type_union() {
        let ann = subscript_expr(
            "Union",
            tuple_expr(vec![name_expr("int"), name_expr("str")]),
        );
        let ts = resolve_ts_type(&ann);
        assert_eq!(ts, TsType::Union(vec![TsType::Number, TsType::String]));
        assert_eq!(ts_type_to_string(&ts), "number | string");
    }

    #[test]
    fn test_ts_type_nested() {
        // List[Optional[int]] → (number | null)[]
        let inner = subscript_expr("Optional", name_expr("int"));
        let ann = subscript_expr("List", inner);
        let ts = resolve_ts_type(&ann);
        assert_eq!(
            ts,
            TsType::Array(Box::new(TsType::Nullable(Box::new(TsType::Number))))
        );
        assert_eq!(ts_type_to_string(&ts), "(number | null)[]");
    }

    #[test]
    fn test_ts_type_named() {
        let ts = resolve_ts_type(&name_expr("MyClass"));
        assert_eq!(ts, TsType::Named("MyClass".to_string()));
        assert_eq!(ts_type_to_string(&ts), "MyClass");
    }

    #[test]
    fn test_ts_type_any_fallback() {
        let ts = resolve_ts_type(&name_expr("unknown_thing"));
        assert_eq!(ts, TsType::Any);
        assert_eq!(ts_type_to_string(&ts), "any");
    }

    #[test]
    fn test_ts_type_callable() {
        // Callable[[int, str], bool]
        let ann = subscript_expr(
            "Callable",
            tuple_expr(vec![
                list_expr(vec![name_expr("int"), name_expr("str")]),
                name_expr("bool"),
            ]),
        );
        let ts = resolve_ts_type(&ann);
        assert_eq!(
            ts,
            TsType::Callable(
                vec![TsType::Number, TsType::String],
                Box::new(TsType::Boolean)
            )
        );
        assert_eq!(
            ts_type_to_string(&ts),
            "(arg0: number, arg1: string) => boolean"
        );
    }

    #[test]
    fn test_ts_type_set() {
        let ann = subscript_expr("Set", name_expr("int"));
        let ts = resolve_ts_type(&ann);
        assert_eq!(ts, TsType::Set(Box::new(TsType::Number)));
        assert_eq!(ts_type_to_string(&ts), "Set<number>");
    }

    // --- Sub-task 2: DTS generator tests ---

    fn compile_dts(source: &str) -> std::string::String {
        let module = pyths_parser::parse(source).expect("Parse failed");
        let mut gen = DtsGenerator::new();
        gen.generate(&module)
    }

    #[test]
    fn test_dts_simple_function() {
        let dts = compile_dts("def add(a: int, b: int) -> int:\n    return a + b");
        assert!(
            dts.contains("export declare function add(a: number, b: number): number;"),
            "DTS: {}",
            dts
        );
    }

    #[test]
    fn test_dts_unannotated_function() {
        // An unannotated function may return a value, so its declared return
        // type is `any` (usable by consumers), NOT `void` (which would break
        // callers that use the result — e.g. `.map()` on a returned list).
        let dts = compile_dts("def foo(x, y):\n    return x + y");
        assert!(
            dts.contains("export declare function foo(x: any, y: any): any;"),
            "DTS: {}",
            dts
        );
    }

    #[test]
    fn test_dts_explicit_none_return_is_void() {
        // `-> None` is a real "returns nothing" claim → void (unchanged).
        let dts = compile_dts("def log(x: str) -> None:\n    print(x)");
        assert!(
            dts.contains("export declare function log(x: string): void;"),
            "DTS: {}",
            dts
        );
    }

    #[test]
    fn test_dts_class_basic() {
        let source = "\
class Counter:
    def __init__(self, start: int = 0):
        self.value = start
    def increment(self) -> None:
        self.value += 1
    def get_value(self) -> int:
        return self.value";
        let dts = compile_dts(source);
        assert!(
            dts.contains("export declare class Counter {"),
            "DTS: {}",
            dts
        );
        assert!(dts.contains("value: number;"), "DTS: {}", dts);
        assert!(dts.contains("constructor(start?: number);"), "DTS: {}", dts);
        assert!(dts.contains("increment(): void;"), "DTS: {}", dts);
        assert!(dts.contains("get_value(): number;"), "DTS: {}", dts);
    }

    #[test]
    fn test_dts_dataclass() {
        let source = "\
@dataclass
class Point:
    x: float
    y: float
    label: str";
        let dts = compile_dts(source);
        assert!(dts.contains("export declare class Point {"), "DTS: {}", dts);
        assert!(dts.contains("x: number;"), "DTS: {}", dts);
        assert!(dts.contains("y: number;"), "DTS: {}", dts);
        assert!(dts.contains("label: string;"), "DTS: {}", dts);
    }

    #[test]
    fn test_dts_async_function() {
        let dts = compile_dts("async def fetch_data(url: str) -> str:\n    pass");
        assert!(
            dts.contains("export declare function fetch_data(url: string): Promise<string>;"),
            "DTS: {}",
            dts
        );
    }

    #[test]
    fn test_dts_annotated_var() {
        let dts = compile_dts("count: int = 0");
        assert!(
            dts.contains("export declare const count: number;"),
            "DTS: {}",
            dts
        );
    }

    #[test]
    fn test_dts_optional_param() {
        let dts = compile_dts("def greet(name: str = \"world\") -> str:\n    return name");
        assert!(
            dts.contains("export declare function greet(name?: string): string;"),
            "DTS: {}",
            dts
        );
    }

    #[test]
    fn test_dts_class_inheritance() {
        let source = "\
class Dog(Animal):
    def speak(self) -> str:
        return \"Woof\"";
        let dts = compile_dts(source);
        assert!(
            dts.contains("export declare class Dog extends Animal {"),
            "DTS: {}",
            dts
        );
    }

    #[test]
    fn test_dts_skips_imports() {
        let source = "from typing import List\ndef foo() -> int:\n    return 1";
        let dts = compile_dts(source);
        assert!(!dts.contains("import"), "DTS should skip imports: {}", dts);
        assert!(
            dts.contains("export declare function foo(): number;"),
            "DTS: {}",
            dts
        );
    }

    #[test]
    fn test_dts_complex_types() {
        // Test nested types via AST directly (parser doesn't support nested generics with commas)
        let inner = subscript_expr("List", subscript_expr("Optional", name_expr("int")));
        let ts = resolve_ts_type(&inner);
        assert_eq!(ts_type_to_string(&ts), "(number | null)[]");

        // Also test via parser: simple generic param + Optional return
        let dts = compile_dts("def process(data: List[int]) -> Optional[str]:\n    pass");
        assert!(dts.contains("data: number[]"), "DTS: {}", dts);
        assert!(dts.contains("string | null"), "DTS: {}", dts);
    }
}
