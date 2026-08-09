use std::collections::HashMap;

/// The kind of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Function,
    Class,
}

/// Information about a symbol in a scope.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    /// Whether this symbol is explicitly declared `global`.
    pub is_global: bool,
    /// Whether this symbol is explicitly declared `nonlocal`.
    pub is_nonlocal: bool,
    /// Whether the symbol is assigned in this scope.
    pub is_assigned: bool,
    /// Whether the symbol is a parameter (function argument).
    pub is_param: bool,
    /// Whether the symbol is imported.
    pub is_imported: bool,
    /// Whether the symbol is referenced (read) anywhere.
    pub is_referenced: bool,
}

impl Default for SymbolInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolInfo {
    pub fn new() -> Self {
        Self {
            is_global: false,
            is_nonlocal: false,
            is_assigned: false,
            is_param: false,
            is_imported: false,
            is_referenced: false,
        }
    }

    /// Returns true if this is a local binding (param, import, or assignment
    /// that is not global/nonlocal).
    pub fn is_local(&self) -> bool {
        !self.is_global
            && !self.is_nonlocal
            && (self.is_assigned || self.is_param || self.is_imported)
    }
}

/// A single lexical scope.
#[derive(Debug, Clone)]
pub struct Scope {
    pub kind: ScopeKind,
    pub symbols: HashMap<String, SymbolInfo>,
    pub parent: Option<usize>,
}

impl Scope {
    pub fn new(kind: ScopeKind, parent: Option<usize>) -> Self {
        Self {
            kind,
            symbols: HashMap::new(),
            parent,
        }
    }

    /// Get or create a symbol entry.
    pub fn get_or_create(&mut self, name: &str) -> &mut SymbolInfo {
        self.symbols.entry(name.to_string()).or_default()
    }

    /// Mark a symbol as assigned.
    pub fn mark_assigned(&mut self, name: &str) {
        self.get_or_create(name).is_assigned = true;
    }

    /// Mark a symbol as a parameter.
    pub fn mark_param(&mut self, name: &str) {
        let info = self.get_or_create(name);
        info.is_param = true;
        info.is_assigned = true;
    }

    /// Mark a symbol as global.
    pub fn mark_global(&mut self, name: &str) {
        self.get_or_create(name).is_global = true;
    }

    /// Mark a symbol as nonlocal.
    pub fn mark_nonlocal(&mut self, name: &str) {
        self.get_or_create(name).is_nonlocal = true;
    }

    /// Mark a symbol as imported.
    pub fn mark_imported(&mut self, name: &str) {
        let info = self.get_or_create(name);
        info.is_imported = true;
        info.is_assigned = true;
    }

    /// Check if a name is known in this scope.
    pub fn has_symbol(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }

    /// Check if a name is a local binding.
    pub fn is_local(&self, name: &str) -> bool {
        self.symbols.get(name).is_some_and(|s| s.is_local())
    }
}
