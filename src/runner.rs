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
    #[inline]
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

/// Env vars safe to forward to git/editor/brew subprocesses.
/// Excludes secrets, session tokens, and injection vectors like GIT_SSH_COMMAND.
const SAFE_ENV_KEYS: &[&str] = &[
    "HOME",
    "PATH",
    "USER",
    "LOGNAME",
    "SHELL",
    "TERM",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "EDITOR",
    "VISUAL",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "HOMEBREW_PREFIX",
    "HOMEBREW_CELLAR",
    "HOMEBREW_REPOSITORY",
    "SSH_AUTH_SOCK",
    "GPG_TTY",
    "GNUPGHOME",
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GIT_COMMITTER_NAME",
    "GIT_COMMITTER_EMAIL",
    "TMPDIR",
    "TZ",
];

impl Runner for SystemRunner {
    fn run(&self, cmd: &str, args: &[&str], cwd: Option<&Path>) -> Result<RunOutput> {
        let mut c = Command::new(cmd);
        c.env_clear();
        for key in SAFE_ENV_KEYS {
            if let Ok(val) = std::env::var(key) {
                c.env(key, val);
            }
        }
        c.args(args);
        if let Some(dir) = cwd {
            c.current_dir(dir);
        }
        let out = c.output()?;
        Ok(RunOutput {
            status: out.status.code().unwrap_or_else(|| {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    out.status.signal().map_or(1, |sig| 128 + sig)
                }
                #[cfg(not(unix))]
                {
                    1
                }
            }),
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
#[derive(Default)]
pub struct MockRunner {
    responses: Vec<(Vec<String>, RunOutput)>,
    allow_unmatched: bool,
}

#[cfg(test)]
impl MockRunner {
    pub fn new() -> Self {
        Self::default()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_output_success_zero() {
        let out = RunOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        };
        assert!(out.success());
    }

    #[test]
    fn run_output_success_nonzero() {
        let out = RunOutput {
            status: 1,
            stdout: String::new(),
            stderr: String::new(),
        };
        assert!(!out.success());
    }

    #[test]
    fn run_output_success_negative() {
        let out = RunOutput {
            status: -1,
            stdout: String::new(),
            stderr: String::new(),
        };
        assert!(!out.success());
    }

    #[test]
    fn mock_runner_returns_registered_response() {
        let runner = MockRunner::new().on("echo", &["hello"], "hello\n", true);
        let out = runner.run("echo", &["hello"], None).unwrap();
        assert!(out.success());
        assert_eq!(out.stdout, "hello\n");
    }

    #[test]
    fn mock_runner_last_match_wins() {
        let runner = MockRunner::new()
            .on("cmd", &[], "first", true)
            .on("cmd", &[], "second", true);
        let out = runner.run("cmd", &[], None).unwrap();
        assert_eq!(out.stdout, "second");
    }

    #[test]
    fn mock_runner_allow_unmatched_returns_success() {
        let runner = MockRunner::new().allow_unmatched();
        let out = runner.run("anything", &["arg"], None).unwrap();
        assert!(out.success());
        assert!(out.stdout.is_empty());
    }

    #[test]
    #[should_panic(expected = "MockRunner: unexpected command")]
    fn mock_runner_panics_on_unmatched() {
        let runner = MockRunner::new();
        let _ = runner.run("unregistered", &[], None);
    }

    #[test]
    fn mock_runner_on_err() {
        let runner = MockRunner::new().on_err("cmd", &[], "error msg", false);
        let out = runner.run("cmd", &[], None).unwrap();
        assert!(!out.success());
        assert_eq!(out.stderr, "error msg");
    }
}
