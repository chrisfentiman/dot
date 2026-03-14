use console::{Style, Term};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::Write;
use std::sync::LazyLock;
use std::time::Duration;

// ── Styles ───────────────────────────────────────────────────────────────────

static ACTION: LazyLock<Style> = LazyLock::new(|| Style::new().green().bold());
static ERROR: LazyLock<Style> = LazyLock::new(|| Style::new().red().bold());
static WARN: LazyLock<Style> = LazyLock::new(|| Style::new().yellow().bold());
static DIM: LazyLock<Style> = LazyLock::new(|| Style::new().dim());
static HIGHLIGHT: LazyLock<Style> = LazyLock::new(|| Style::new().cyan());
static BOLD: LazyLock<Style> = LazyLock::new(|| Style::new().bold());

const WIDTH: usize = 10;

// ── UI ───────────────────────────────────────────────────────────────────────

pub struct UI {
    term: Term,
}

impl Default for UI {
    fn default() -> Self {
        Self::new()
    }
}

impl UI {
    /// Create a new UI that writes to stderr.
    pub fn new() -> Self {
        Self {
            term: Term::stderr(),
        }
    }

    /// `  dotf v0.9.0`
    pub fn header(&self) {
        let version = env!("CARGO_PKG_VERSION");
        let _ = writeln!(&self.term);
        let _ = writeln!(
            &self.term,
            "{:>WIDTH$} {}",
            BOLD.apply_to("dotf"),
            DIM.apply_to(format!("v{version}"))
        );
        let _ = writeln!(&self.term);
    }

    /// Green bold verb, right-aligned.
    /// ```text
    ///    Checking ~/.dotf
    /// ```
    pub fn action(&self, verb: &str, message: impl std::fmt::Display) {
        let _ = writeln!(&self.term, "{:>WIDTH$} {}", ACTION.apply_to(verb), message);
    }

    /// Dim verb for skipped/unchanged items.
    /// ```text
    ///    Skipped starship.toml (unchanged)
    /// ```
    pub fn skip(&self, verb: &str, message: impl std::fmt::Display) {
        let _ = writeln!(
            &self.term,
            "{:>WIDTH$} {}",
            DIM.apply_to(verb),
            DIM.apply_to(message)
        );
    }

    /// Yellow bold verb for warnings.
    /// ```text
    ///    Warning ~/dotfiles exists — dotf now uses ~/.dotf
    /// ```
    pub fn warn(&self, verb: &str, message: impl std::fmt::Display) {
        let _ = writeln!(&self.term, "{:>WIDTH$} {}", WARN.apply_to(verb), message);
    }

    /// Red bold verb for errors.
    /// ```text
    ///      Error git push failed
    /// ```
    pub fn error(&self, verb: &str, message: impl std::fmt::Display) {
        let _ = writeln!(&self.term, "{:>WIDTH$} {}", ERROR.apply_to(verb), message);
    }

    /// Green bold "Finished" line with optional detail.
    /// ```text
    ///   Finished sync — 2 updated, 1 unchanged
    /// ```
    pub fn finished(&self, message: impl std::fmt::Display) {
        let _ = writeln!(
            &self.term,
            "{:>WIDTH$} {}",
            ACTION.apply_to("Finished"),
            message
        );
        let _ = writeln!(&self.term);
    }

    /// Highlighted value (cyan) for use in format strings.
    pub fn highlight<D: std::fmt::Display>(&self, val: D) -> String {
        HIGHLIGHT.apply_to(val).to_string()
    }

    /// Bold value for use in format strings.
    pub fn bold<D: std::fmt::Display>(&self, val: D) -> String {
        BOLD.apply_to(val).to_string()
    }

    /// Dim value for use in format strings.
    pub fn dim<D: std::fmt::Display>(&self, val: D) -> String {
        DIM.apply_to(val).to_string()
    }

    /// Print a blank line.
    pub fn blank(&self) {
        let _ = writeln!(&self.term);
    }

    /// Print a raw line to stderr (for passthrough content like git output).
    pub fn raw(&self, line: impl std::fmt::Display) {
        let _ = writeln!(&self.term, "{line}");
    }

    /// Start a spinner for a slow/async operation. The spinner animates
    /// immediately — if the operation completes fast, call `.finish()` and
    /// it disappears cleanly.
    pub fn spinner(&self, message: impl Into<String>) -> Spinner<'_> {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template(&format!(
                "{{spinner:.cyan}} {{msg:>{w}}}",
                w = WIDTH - 2 // account for spinner + space
            ))
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_message(message.into());
        Spinner { pb, ui: self }
    }

    // ── Table helpers ────────────────────────────────────────────────────

    /// Print a table header row with bold column names.
    pub fn table_header(&self, columns: &[(&str, usize)]) {
        let mut line = String::new();
        for (i, (name, width)) in columns.iter().enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            line.push_str(&format!("{:<width$}", BOLD.apply_to(name), width = width));
        }
        let _ = writeln!(&self.term, "{line}");
    }

    /// Print a table separator.
    pub fn table_separator(&self, width: usize) {
        let _ = writeln!(&self.term, "{}", DIM.apply_to("─".repeat(width)));
    }

    /// Print a table row. Values are pre-formatted by the caller.
    pub fn table_row(&self, line: impl std::fmt::Display) {
        let _ = writeln!(&self.term, "{line}");
    }

    // ── Status symbols ───────────────────────────────────────────────────

    pub fn sym_ok(&self) -> String {
        ACTION.apply_to("✓").to_string()
    }

    pub fn sym_err(&self) -> String {
        ERROR.apply_to("✗").to_string()
    }

    pub fn sym_warn(&self) -> String {
        WARN.apply_to("⚠").to_string()
    }

    pub fn sym_dim(&self) -> String {
        DIM.apply_to("·").to_string()
    }

    /// Indented continuation line (same padding as a verb line, no verb).
    pub fn hint(&self, message: impl std::fmt::Display) {
        let _ = writeln!(&self.term, "{:>WIDTH$} {}", "", DIM.apply_to(message));
    }
}

// ── Spinner ──────────────────────────────────────────────────────────────────

pub struct Spinner<'a> {
    pb: ProgressBar,
    ui: &'a UI,
}

impl Spinner<'_> {
    /// Complete the spinner and print a green action line in its place.
    pub fn finish(self, verb: &str, message: impl std::fmt::Display) {
        self.pb.finish_and_clear();
        self.ui.action(verb, message);
    }

    /// Complete the spinner and print a skip line.
    pub fn finish_skip(self, verb: &str, message: impl std::fmt::Display) {
        self.pb.finish_and_clear();
        self.ui.skip(verb, message);
    }

    /// Complete the spinner and print a warning line.
    pub fn finish_warn(self, verb: &str, message: impl std::fmt::Display) {
        self.pb.finish_and_clear();
        self.ui.warn(verb, message);
    }

    /// Update the spinner message mid-operation.
    pub fn set_message(&self, message: impl Into<String>) {
        self.pb.set_message(message.into());
    }
}
