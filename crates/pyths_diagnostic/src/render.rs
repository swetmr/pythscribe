use ariadne::{Color, Label, Report, ReportKind, Source};

use crate::errors::{Diagnostic, Severity};

/// Render a diagnostic to a string using ariadne.
pub fn render_diagnostic(source: &str, filename: &str, diag: &Diagnostic) -> String {
    let kind = match diag.severity {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
        Severity::Info => ReportKind::Advice,
    };

    let mut builder = Report::build(kind, filename, diag.span.start).with_message(&diag.message);

    // Add the primary span label
    builder = builder.with_label(
        Label::new((filename, diag.span.start..diag.span.end))
            .with_message(&diag.message)
            .with_color(Color::Red),
    );

    // Add additional labels
    for (i, label) in diag.labels.iter().enumerate() {
        let color = if i == 0 { Color::Blue } else { Color::Yellow };
        builder = builder.with_label(
            Label::new((filename, label.span.start..label.span.end))
                .with_message(&label.message)
                .with_color(color),
        );
    }

    // Add notes
    for note in &diag.notes {
        builder = builder.with_note(note);
    }

    let report = builder.finish();
    let mut output = Vec::new();
    report
        .write((filename, Source::from(source)), &mut output)
        .expect("Failed to write diagnostic");

    String::from_utf8(output).expect("Invalid UTF-8 in diagnostic output")
}

/// Render multiple diagnostics.
pub fn render_diagnostics(source: &str, filename: &str, diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|d| render_diagnostic(source, filename, d))
        .collect::<Vec<_>>()
        .join("\n")
}
