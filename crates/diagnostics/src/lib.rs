use std::fmt;
use std::io;
use std::ops::Range;

use ariadne::Color;
use ariadne::Label as AriadneLabel;
use ariadne::Report as AriadneReport;
use ariadne::ReportKind as AriadneReportKind;
use ariadne::Source;
pub use yansi::Paint;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self { Self { start, end } }
    fn clamp(self, source: &str) -> Range<usize> {
        let start = self.start.min(source.len());
        let end = self.end.min(source.len());

        if start < end {
            start..end
        } else {
            start..start
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl<'a> From<Severity> for AriadneReportKind<'a> {
    fn from(value: Severity) -> Self {
        match value {
            Severity::Info => Self::Advice,
            Severity::Warning => Self::Warning,
            Severity::Error => Self::Error,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    Lexer,
    Parser,
    Semantics,
    Verification,
}

impl fmt::Display for DiagnosticKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticKind::Lexer => write!(f, "Lexical error"),
            DiagnosticKind::Parser => write!(f, "Parser error"),
            DiagnosticKind::Semantics => write!(f, "Semantic error"),
            DiagnosticKind::Verification => write!(f, "Verification error"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub kind: DiagnosticKind,
    pub severity: Severity,
    pub span: Span,
    pub message: String,
    pub labels: Vec<Label>,
    pub note: Option<String>,
}

impl Report {
    pub fn new(
        kind: DiagnosticKind,
        severity: Severity,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            severity,
            span,
            message: message.into(),
            labels: Vec::new(),
            note: None,
        }
    }

    pub fn lexer(span: Span, message: impl Into<String>) -> Self {
        Self::new(DiagnosticKind::Lexer, Severity::Error, span, message)
            .with_note("change the token so it belongs to the May grammar")
    }

    pub fn parser(span: Span, message: impl Into<String>) -> Self {
        Self::new(DiagnosticKind::Parser, Severity::Error, span, message)
            .with_note("change the syntax so it matches the May grammar")
    }

    pub fn semantics(span: Span, message: impl Into<String>) -> Self {
        Self::new(DiagnosticKind::Semantics, Severity::Error, span, message)
            .with_note("change the declaration or expression so it is semantically valid")
    }

    pub fn verification(span: Span, message: impl Into<String>) -> Self {
        Self::new(DiagnosticKind::Verification, Severity::Error, span, message)
            .with_note("review the bound that produced this verification error")
    }

    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

pub trait ToReport {
    fn to_report(&self) -> Report;
}

pub fn render_reports(file_name: &str, source: &str, reports: &[Report]) -> io::Result<()> {
    for report in reports {
        render_report(file_name, source, report)?;
    }

    Ok(())
}

fn render_report(file_name: &str, source: &str, report: &Report) -> io::Result<()> {
    let title = format!("{} detected.", report.kind);
    let mut builder = AriadneReport::build(report.severity.into(), file_name, report.span.start)
        .with_message(title)
        .with_label(
            AriadneLabel::new((file_name, report.span.clamp(source)))
                .with_message(report.message.clone())
                .with_color(Color::Yellow),
        );

    for label in &report.labels {
        builder = builder.with_label(
            AriadneLabel::new((file_name, label.span.clamp(source)))
                .with_message(label.message.clone())
                .with_color(Color::Yellow),
        );
    }

    if let Some(note) = &report.note {
        builder = builder.with_note(note.clone());
    }

    builder.finish().print((file_name, Source::from(source)))
}

#[cfg(test)]
mod tests {
    use super::DiagnosticKind;
    use super::Report;
    use super::Severity;
    use super::Span;

    #[test]
    fn helper_reports_keep_their_kind_and_note() {
        let report = Report::parser(Span::new(1, 4), "unexpected token");

        assert_eq!(report.kind, DiagnosticKind::Parser);
        assert_eq!(report.severity, Severity::Error);
        assert_eq!(report.span, Span::new(1, 4));
        assert_eq!(report.message, "unexpected token");
        assert!(report.note.is_some());
    }

    #[test]
    fn spans_are_clamped_to_source_length() {
        assert_eq!(Span::new(2, 10).clamp("abcd"), 2..4);
        assert_eq!(Span::new(10, 20).clamp("abcd"), 4..4);
    }
}
