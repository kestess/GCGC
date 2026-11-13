//! Captured console output.
//!
//! The original prints warnings and ASCII tables with `print`. Analyses write the same text here,
//! so it can be compared with the Python output and shown in the web UI.

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Out {
    /// Exactly what the Python version would print to stdout.
    pub stdout: String,
    /// Extra diagnostics the Python version does not print (e.g. unparseable numbers).
    pub diagnostics: Vec<String>,
}

impl Out {
    pub fn new() -> Self {
        Self::default()
    }

    /// `print(s)`
    pub fn println(&mut self, s: &str) {
        self.stdout.push_str(s);
        self.stdout.push('\n');
    }

    /// `print(s, end="")`
    pub fn print(&mut self, s: &str) {
        self.stdout.push_str(s);
    }

    pub fn diag(&mut self, s: String) {
        self.diagnostics.push(s);
    }
}
