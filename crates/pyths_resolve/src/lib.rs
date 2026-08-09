// PythScribe name resolution — Phase 1
// LEGB scope analysis, symbol table, tracks variable declarations vs reassignments.

pub mod resolve;
pub mod scope;

pub use resolve::{resolve, ResolveInfo};
pub use scope::{ScopeKind, SymbolInfo};
