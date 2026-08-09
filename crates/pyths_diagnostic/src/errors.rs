use pyths_syntax::span::Span;

/// Severity level for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A diagnostic message with source location.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
}

/// A labeled span within a diagnostic.
#[derive(Debug, Clone)]
pub struct Label {
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span,
            labels: vec![],
            notes: vec![],
        }
    }

    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            span,
            labels: vec![],
            notes: vec![],
        }
    }

    pub fn with_label(mut self, message: impl Into<String>, span: Span) -> Self {
        self.labels.push(Label {
            message: message.into(),
            span,
        });
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}
