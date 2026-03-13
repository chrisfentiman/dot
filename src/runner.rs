use anyhow::Result;
use std::path::Path;
use std::process::Command;

/// Decoded output from a subprocess invocation.
#[derive(Debug, Clone)]
pub struct RunOutput {
    /// Exit code; 0 = success.
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl RunOutput {
    #[must_use]
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

/// Injectable subprocess runner for testability.
pub trait Runner {
    fn run(&self, cmd: &str, args: &[&str], cwd: Option<&Path>) -> Result<RunOutput>;
}

// ── Real implementation ──────────────────────────────────────────────────────

pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(&self, cmd: &str, args: &[&str], cwd: Option<&Path>) -> Result<RunOutput> {
        let mut c = Command::new(cmd);
        c.args(args);
        if let Some(dir) = cwd {
            c.current_dir(dir);
        }
        let out = c.output()?;
        Ok(RunOutput {
            status: out.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

// ── Test mock ────────────────────────────────────────────────────────────────

/// Canned-response runner for unit tests.
///
/// Keys are `(cmd, args)` tuples — no string-joining, so args with spaces or
/// special characters match correctly.
///
/// By default, any command that was not registered with `on`/`on_err` will
/// `panic!` with a clear message. Call `.allow_unmatched()` if the test
/// genuinely does not care about extra commands.
#[cfg(test)]
pub struct MockRunner {
    responses: Vec<(Vec<String>, RunOutput)>,
    allow_unmatched: bool,
}

#[cfg(test)]
impl MockRunner {
    pub fn new() -> Self {
        Self {
            responses: Vec::new(),
            allow_unmatched: false,
        }
    }

    /// When called, unmatched commands return success with empty output
    /// instead of panicking. Use for tests that don't care about extra calls.
    pub fn allow_unmatched(mut self) -> Self {
        self.allow_unmatched = true;
        self
    }

    fn make_key(cmd: &str, args: &[&str]) -> Vec<String> {
        std::iter::once(cmd)
            .chain(args.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    /// Register a canned stdout response.
    pub fn on(mut self, cmd: &str, args: &[&str], stdout: &str, success: bool) -> Self {
        self.responses.push((
            Self::make_key(cmd, args),
            RunOutput {
                status: if success { 0 } else { 1 },
                stdout: stdout.to_owned(),
                stderr: String::new(),
            },
        ));
        self
    }

    /// Register a canned stderr response (failed by default).
    pub fn on_err(mut self, cmd: &str, args: &[&str], stderr: &str, success: bool) -> Self {
        self.responses.push((
            Self::make_key(cmd, args),
            RunOutput {
                status: if success { 0 } else { 1 },
                stdout: String::new(),
                stderr: stderr.to_owned(),
            },
        ));
        self
    }
}

#[cfg(test)]
impl Runner for MockRunner {
    fn run(&self, cmd: &str, args: &[&str], _cwd: Option<&Path>) -> Result<RunOutput> {
        let key = MockRunner::make_key(cmd, args);
        // Search in registration order; last matching wins if registered twice
        for (k, v) in self.responses.iter().rev() {
            if *k == key {
                return Ok(v.clone());
            }
        }
        if self.allow_unmatched {
            return Ok(RunOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            });
        }
        panic!("MockRunner: unexpected command: {:?} {:?}", cmd, args);
    }
}
