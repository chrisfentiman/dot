use anyhow::{Result, anyhow};
use std::process::Command;
use zeroize::Zeroizing;

/// Truncate CLI stderr to a reasonable length for error messages.
/// Returns a `Cow` to avoid allocation when the input is already short enough.
pub(crate) fn truncate_stderr(stderr: &str, max_bytes: usize) -> std::borrow::Cow<'_, str> {
    let trimmed = stderr.trim();
    if trimmed.len() <= max_bytes {
        std::borrow::Cow::Borrowed(trimmed)
    } else {
        let truncated = &trimmed[..trimmed.floor_char_boundary(max_bytes)];
        std::borrow::Cow::Owned(format!("{truncated}… (truncated)"))
    }
}

/// Build the minimal environment to forward to a child process.
/// Always includes `HOME` and `PATH`; caller may request additional named vars.
fn forwarded_env(extra: &[&str]) -> Vec<(String, String)> {
    let mut env = Vec::new();
    for key in ["HOME", "PATH"].iter().chain(extra.iter()) {
        if let Ok(val) = std::env::var(key) {
            env.push((key.to_string(), val));
        }
    }
    env
}

// ── SecretRunner abstraction ────────────────────────────────────────────────

/// Output from a secret-fetching CLI command.
pub(crate) struct SecretOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Abstraction over CLI invocation for secret-fetching commands.
/// Allows tests to inject canned responses without spawning real processes.
pub(crate) trait SecretRunner {
    fn run_cli(
        &self,
        cmd: &str,
        args: &[&str],
        env: Vec<(String, String)>,
    ) -> std::io::Result<SecretOutput>;
}

/// Production implementation: spawns a real subprocess with env_clear.
struct RealSecretRunner;

impl SecretRunner for RealSecretRunner {
    fn run_cli(
        &self,
        cmd: &str,
        args: &[&str],
        env: Vec<(String, String)>,
    ) -> std::io::Result<SecretOutput> {
        let output = Command::new(cmd)
            .env_clear()
            .envs(forwarded_env(&[]))
            .envs(env)
            .args(args)
            .output()?;
        Ok(SecretOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Fetch a secret value from any supported backend based on URI scheme.
///
/// Supported schemes:
///   pass://vault/item/field   — Proton Pass CLI (`pass`)
///   op://vault/item/field     — 1Password CLI (`op`)
///   bw://item-name/field      — Bitwarden CLI (`bw`, requires BW_SESSION env var)
///   env://VAR_NAME            — environment variable (useful for CI)
pub fn fetch(uri: &str) -> Result<Zeroizing<String>> {
    fetch_with(uri, &RealSecretRunner)
}

/// Return a human-readable backend name for a URI.
#[must_use]
pub fn backend_name(uri: &str) -> &'static str {
    if uri.starts_with("pass://") {
        "Proton Pass"
    } else if uri.starts_with("op://") {
        "1Password"
    } else if uri.starts_with("bw://") {
        "Bitwarden"
    } else if uri.starts_with("env://") {
        "environment"
    } else {
        "unknown"
    }
}

// ── Internal dispatch ───────────────────────────────────────────────────────

fn fetch_with(uri: &str, runner: &dyn SecretRunner) -> Result<Zeroizing<String>> {
    if let Some(path) = uri.strip_prefix("pass://") {
        fetch_pass_with(path, uri, runner)
    } else if let Some(path) = uri.strip_prefix("op://") {
        fetch_op_with(path, uri, runner)
    } else if let Some(path) = uri.strip_prefix("bw://") {
        fetch_bw_with(path, uri, runner)
    } else if let Some(var) = uri.strip_prefix("env://") {
        fetch_env(var)
    } else {
        Err(anyhow!(
            "Unknown secret URI scheme: '{}'\n  Supported: pass://, op://, bw://, env://",
            uri
        ))
    }
}

/// Run a secret-fetching CLI command and return its stdout as a zeroized string.
/// Handles stderr truncation+zeroization, UTF-8 validation, and
/// trailing newline stripping — the shared logic for all CLI-based backends.
fn run_secret_cli(
    runner: &dyn SecretRunner,
    cmd: &str,
    args: &[&str],
    extra_env: Vec<(String, String)>,
    friendly_name: &str,
    install_hint: &str,
    original_uri: &str,
) -> Result<Zeroizing<String>> {
    let output = runner
        .run_cli(cmd, args, extra_env)
        .map_err(|e| anyhow!("Failed to run {friendly_name} (`{cmd}`): {e}\n  {install_hint}"))?;

    if !output.success {
        let lossy = Zeroizing::new(String::from_utf8_lossy(&output.stderr).into_owned());
        let stderr = truncate_stderr(&lossy, 512);
        return Err(anyhow!(
            "{friendly_name} failed for {original_uri}: {stderr}"
        ));
    }

    let mut value = Zeroizing::new(String::from_utf8(output.stdout).map_err(|e| {
        let mut bytes = e.into_bytes();
        zeroize::Zeroize::zeroize(&mut bytes);
        anyhow!("{friendly_name} output for {original_uri} is not valid UTF-8")
    })?);
    let trimmed_len = value.trim_end_matches(['\n', '\r']).len();
    value.truncate(trimmed_len);
    Ok(value)
}

fn fetch_pass_with(
    path: &str,
    original_uri: &str,
    runner: &dyn SecretRunner,
) -> Result<Zeroizing<String>> {
    let full_uri = format!("pass://{path}");
    run_secret_cli(
        runner,
        "pass",
        &["item", "get", "--fields", "password", "--", &full_uri],
        Vec::new(),
        "Proton Pass",
        "Install: brew install protonpass/pass/pass",
        original_uri,
    )
}

fn fetch_op_with(
    path: &str,
    original_uri: &str,
    runner: &dyn SecretRunner,
) -> Result<Zeroizing<String>> {
    let full_uri = format!("op://{path}");
    // Forward only OP_SESSION_<account> and OP_SERVICE_ACCOUNT_TOKEN — the op CLI
    // requires these for non-interactive auth. The length limit (≤40 total chars)
    // and charset restriction prevent abuse via crafted env var names.
    let op_vars: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| {
            k == "OP_SERVICE_ACCOUNT_TOKEN"
                || (k.starts_with("OP_SESSION_")
                    && k.len() <= 40
                    && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        })
        .collect();
    run_secret_cli(
        runner,
        "op",
        &["read", "--", &full_uri],
        op_vars,
        "1Password",
        "Install: brew install 1password-cli",
        original_uri,
    )
}

fn fetch_bw_with(
    path: &str,
    original_uri: &str,
    runner: &dyn SecretRunner,
) -> Result<Zeroizing<String>> {
    let (item, field) = match path.rsplit_once('/') {
        Some((item, field)) if !item.is_empty() => (item, field),
        Some(("", _)) => {
            return Err(anyhow!(
                "Bitwarden: item name cannot be empty in {original_uri}"
            ));
        }
        _ => (path, "password"),
    };

    let subcommand = match field {
        "password" | "" => "password",
        "username" => "username",
        "notes" => "notes",
        "uri" => "uri",
        other => {
            return Err(anyhow!(
                "Bitwarden: unknown field '{}' in {original_uri}\n  Supported fields: password, username, notes, uri",
                other
            ));
        }
    };

    run_secret_cli(
        runner,
        "bw",
        &["get", subcommand, "--", item],
        forwarded_env(&["BW_SESSION"]),
        "Bitwarden",
        "Install: brew install bitwarden-cli\n  Requires BW_SESSION env var (run: export BW_SESSION=$(bw unlock --raw))",
        original_uri,
    )
}

fn fetch_env(var: &str) -> Result<Zeroizing<String>> {
    std::env::var(var)
        .map(Zeroizing::new)
        .map_err(|_| anyhow!("Environment variable '{}' is not set", var))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    // ── MockSecretRunner ────────────────────────────────────────

    struct MockSecretRunner {
        responses: Vec<(
            String,
            Vec<String>,
            Result<SecretOutput, std::io::ErrorKind>,
        )>,
        captured_envs: RefCell<Vec<Vec<(String, String)>>>,
    }

    impl MockSecretRunner {
        fn new() -> Self {
            Self {
                responses: Vec::new(),
                captured_envs: RefCell::new(Vec::new()),
            }
        }

        fn on_success(mut self, cmd: &str, args: &[&str], stdout: &[u8]) -> Self {
            self.responses.push((
                cmd.to_string(),
                args.iter().map(|s| s.to_string()).collect(),
                Ok(SecretOutput {
                    success: true,
                    stdout: stdout.to_vec(),
                    stderr: Vec::new(),
                }),
            ));
            self
        }

        fn on_failure(mut self, cmd: &str, args: &[&str], stderr: &[u8]) -> Self {
            self.responses.push((
                cmd.to_string(),
                args.iter().map(|s| s.to_string()).collect(),
                Ok(SecretOutput {
                    success: false,
                    stdout: Vec::new(),
                    stderr: stderr.to_vec(),
                }),
            ));
            self
        }

        fn on_io_error(mut self, cmd: &str, args: &[&str], kind: std::io::ErrorKind) -> Self {
            self.responses.push((
                cmd.to_string(),
                args.iter().map(|s| s.to_string()).collect(),
                Err(kind),
            ));
            self
        }
    }

    impl SecretRunner for MockSecretRunner {
        fn run_cli(
            &self,
            cmd: &str,
            args: &[&str],
            env: Vec<(String, String)>,
        ) -> std::io::Result<SecretOutput> {
            self.captured_envs.borrow_mut().push(env);
            let args_vec: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            for (c, a, r) in self.responses.iter().rev() {
                if c == cmd && *a == args_vec {
                    return match r {
                        Ok(out) => Ok(SecretOutput {
                            success: out.success,
                            stdout: out.stdout.clone(),
                            stderr: out.stderr.clone(),
                        }),
                        Err(kind) => Err(std::io::Error::new(*kind, "mock IO error")),
                    };
                }
            }
            panic!("MockSecretRunner: unexpected call: {cmd} {args:?}");
        }
    }

    // ── backend_name ─────────────────────────────────────────────
    #[test]
    fn backend_name_known_schemes() {
        assert_eq!(backend_name("pass://vault/item/field"), "Proton Pass");
        assert_eq!(backend_name("op://vault/item/field"), "1Password");
        assert_eq!(backend_name("bw://item/field"), "Bitwarden");
        assert_eq!(backend_name("env://MY_VAR"), "environment");
    }

    #[test]
    fn backend_name_unknown_returns_unknown() {
        assert_eq!(backend_name("vault://secret"), "unknown");
        assert_eq!(backend_name(""), "unknown");
        assert_eq!(backend_name("http://example.com"), "unknown");
    }

    // ── fetch: unknown / empty scheme ─────────────────────────────
    #[test]
    fn fetch_unknown_scheme_errors() {
        let err = fetch("vault://some/path").unwrap_err();
        assert!(err.to_string().contains("Unknown secret URI scheme"));
    }

    #[test]
    fn fetch_empty_uri_errors() {
        let err = fetch("").unwrap_err();
        assert!(err.to_string().contains("Unknown secret URI scheme"));
    }

    #[test]
    fn fetch_no_double_slash_errors() {
        // "env:MY_VAR" is not a valid URI — should hit unknown scheme
        let err = fetch("env:MY_VAR").unwrap_err();
        assert!(err.to_string().contains("Unknown secret URI scheme"));
    }

    // ── fetch: env backend ───────────────────────────────────────
    #[test]
    fn fetch_env_present() {
        let _g = crate::env_lock();
        let _secret = crate::EnvGuard::set("_DOTF_TEST_SECRET", "hunter2");
        let val = fetch("env://_DOTF_TEST_SECRET").unwrap();
        assert_eq!(val.as_str(), "hunter2");
    }

    #[test]
    fn fetch_env_missing_errors() {
        let _g = crate::env_lock();
        unsafe {
            std::env::remove_var("_DOTF_TEST_MISSING_XYZ");
        }
        let err = fetch("env://_DOTF_TEST_MISSING_XYZ").unwrap_err();
        assert!(err.to_string().contains("_DOTF_TEST_MISSING_XYZ"));
    }

    #[test]
    fn fetch_env_empty_var_name_errors() {
        let err = fetch("env://").unwrap_err();
        assert!(err.to_string().contains("is not set"));
    }

    // ── fetch: bw routing (CLI absent in CI, error should name bw) ─
    #[test]
    fn fetch_bw_routes_to_bitwarden() {
        let err = fetch("bw://myitem/username").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Bitwarden") || msg.contains("`bw`"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn fetch_bw_no_field_routes_to_bitwarden() {
        let err = fetch("bw://myitem").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Bitwarden") || msg.contains("`bw`"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn fetch_bw_unknown_field_errors() {
        let err = fetch("bw://myitem/custom_field").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field") && msg.contains("custom_field"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn fetch_bw_empty_item_name_errors() {
        let err = fetch("bw:///password").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("item name cannot be empty"),
            "unexpected error: {msg}"
        );
    }

    // ── fetch: pass routing ──────────────────────────────────────
    #[test]
    fn fetch_pass_routes_to_proton_pass() {
        let err = fetch("pass://vault/item/field").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Proton Pass") || msg.contains("`pass`"),
            "unexpected error: {msg}"
        );
    }

    // ── fetch: op routing ────────────────────────────────────────
    #[test]
    fn fetch_op_routes_to_1password() {
        let err = fetch("op://vault/item/field").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("1Password") || msg.contains("`op`"),
            "unexpected error: {msg}"
        );
    }

    // ── truncate_stderr ─────────────────────────────────────────
    #[test]
    fn truncate_stderr_short_passthrough() {
        let result = truncate_stderr("short error", 512);
        assert_eq!(&*result, "short error");
        assert!(
            matches!(result, std::borrow::Cow::Borrowed(_)),
            "short input should borrow"
        );
    }

    #[test]
    fn truncate_stderr_trims_whitespace() {
        let result = truncate_stderr("  error\n\n", 512);
        assert_eq!(&*result, "error");
    }

    #[test]
    fn truncate_stderr_truncates_long() {
        let long = "x".repeat(1000);
        let result = truncate_stderr(&long, 10);
        assert!(result.len() < 30, "should be truncated: {}", result.len());
        assert!(result.contains("(truncated)"));
        assert!(result.starts_with("xxxxxxxxxx"));
        assert!(
            matches!(result, std::borrow::Cow::Owned(_)),
            "truncated should be owned"
        );
    }

    #[test]
    fn truncate_stderr_empty() {
        let result = truncate_stderr("", 512);
        assert_eq!(&*result, "");
    }

    // ── forwarded_env ───────────────────────────────────────────
    #[test]
    fn forwarded_env_includes_home_and_path() {
        let _g = crate::env_lock();
        // Ensure HOME is set for this test (guard restores original on drop).
        let _home = if std::env::var("HOME").is_err() {
            Some(crate::EnvGuard::set("HOME", "/tmp"))
        } else {
            None
        };
        let env = forwarded_env(&[]);
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"HOME"), "should include HOME");
        assert!(keys.contains(&"PATH"), "should include PATH");
    }

    #[test]
    fn forwarded_env_includes_extras() {
        let _g = crate::env_lock();
        let _fwd = crate::EnvGuard::set("_DOTF_FWD_TEST", "val");
        let env = forwarded_env(&["_DOTF_FWD_TEST"]);
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"_DOTF_FWD_TEST"));
    }

    #[test]
    fn forwarded_env_missing_extra_skipped() {
        let _g = crate::env_lock();
        unsafe {
            std::env::remove_var("_DOTF_FWD_ABSENT");
        }
        let env = forwarded_env(&["_DOTF_FWD_ABSENT"]);
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert!(!keys.contains(&"_DOTF_FWD_ABSENT"));
    }

    // ── run_secret_cli via MockSecretRunner ─────────────────────

    #[test]
    fn run_secret_cli_success_strips_newlines() {
        let runner = MockSecretRunner::new().on_success("mycli", &["arg1"], b"secret-value\n\n");
        let result = run_secret_cli(
            &runner,
            "mycli",
            &["arg1"],
            vec![],
            "TestCLI",
            "install hint",
            "test://uri",
        )
        .unwrap();
        assert_eq!(result.as_str(), "secret-value");
    }

    #[test]
    fn run_secret_cli_strips_cr_and_lf() {
        let runner = MockSecretRunner::new().on_success("mycli", &[], b"value\r\n");
        let result = run_secret_cli(
            &runner,
            "mycli",
            &[],
            vec![],
            "TestCLI",
            "hint",
            "test://uri",
        )
        .unwrap();
        assert_eq!(result.as_str(), "value");
    }

    #[test]
    fn run_secret_cli_failure_includes_stderr() {
        let runner = MockSecretRunner::new().on_failure("mycli", &["arg1"], b"auth failed");
        let err = run_secret_cli(
            &runner,
            "mycli",
            &["arg1"],
            vec![],
            "TestCLI",
            "install hint",
            "test://uri",
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("auth failed"), "should include stderr: {msg}");
        assert!(
            msg.contains("TestCLI"),
            "should include friendly name: {msg}"
        );
    }

    #[test]
    fn run_secret_cli_non_utf8_errors() {
        let runner = MockSecretRunner::new().on_success("mycli", &[], &[0xFF, 0xFE]);
        let err = run_secret_cli(
            &runner,
            "mycli",
            &[],
            vec![],
            "TestCLI",
            "hint",
            "test://uri",
        )
        .unwrap_err();
        assert!(err.to_string().contains("not valid UTF-8"));
    }

    #[test]
    fn run_secret_cli_io_error_includes_hint() {
        let runner =
            MockSecretRunner::new().on_io_error("mycli", &[], std::io::ErrorKind::NotFound);
        let err = run_secret_cli(
            &runner,
            "mycli",
            &[],
            vec![],
            "TestCLI",
            "install: brew install mycli",
            "test://uri",
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("TestCLI"),
            "should include friendly name: {msg}"
        );
        assert!(
            msg.contains("install: brew install mycli"),
            "should include hint: {msg}"
        );
    }

    // ── fetch_pass_with ─────────────────────────────────────────

    #[test]
    fn fetch_pass_with_constructs_correct_args() {
        let runner = MockSecretRunner::new().on_success(
            "pass",
            &[
                "item",
                "get",
                "--fields",
                "password",
                "--",
                "pass://vault/item/field",
            ],
            b"s3cret\n",
        );
        let result =
            fetch_pass_with("vault/item/field", "pass://vault/item/field", &runner).unwrap();
        assert_eq!(result.as_str(), "s3cret");
    }

    // ── fetch_op_with ───────────────────────────────────────────

    #[test]
    fn fetch_op_with_constructs_correct_args() {
        let runner = MockSecretRunner::new().on_success(
            "op",
            &["read", "--", "op://vault/item/field"],
            b"op-secret\n",
        );
        let result = fetch_op_with("vault/item/field", "op://vault/item/field", &runner).unwrap();
        assert_eq!(result.as_str(), "op-secret");
    }

    #[test]
    fn fetch_op_with_forwards_session_vars() {
        let _g = crate::env_lock();
        let _session = crate::EnvGuard::set("OP_SESSION_myacct", "tok123");
        let _sa_tok = crate::EnvGuard::set("OP_SERVICE_ACCOUNT_TOKEN", "sa-tok");

        let runner =
            MockSecretRunner::new().on_success("op", &["read", "--", "op://v/i/f"], b"val");
        fetch_op_with("v/i/f", "op://v/i/f", &runner).unwrap();

        let envs = runner.captured_envs.borrow();
        assert_eq!(envs.len(), 1);
        let env_keys: Vec<&str> = envs[0].iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            env_keys.contains(&"OP_SESSION_myacct"),
            "should forward OP_SESSION_*"
        );
        assert!(
            env_keys.contains(&"OP_SERVICE_ACCOUNT_TOKEN"),
            "should forward OP_SERVICE_ACCOUNT_TOKEN"
        );
    }

    #[test]
    fn fetch_op_with_rejects_long_session_var() {
        let _g = crate::env_lock();
        // 41 chars — exceeds the <= 40 length limit
        let long_key = format!("OP_SESSION_{}", "x".repeat(30));
        let _long = crate::EnvGuard::set(&long_key, "val");

        let runner =
            MockSecretRunner::new().on_success("op", &["read", "--", "op://v/i/f"], b"val");
        fetch_op_with("v/i/f", "op://v/i/f", &runner).unwrap();

        let envs = runner.captured_envs.borrow();
        let env_keys: Vec<&str> = envs[0].iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            !env_keys.contains(&long_key.as_str()),
            "should not forward overly long OP_SESSION_ var"
        );
    }

    // ── fetch_bw_with ───────────────────────────────────────────

    #[test]
    fn fetch_bw_with_password_field() {
        let runner = MockSecretRunner::new().on_success(
            "bw",
            &["get", "password", "--", "myitem"],
            b"pw123\n",
        );
        let result = fetch_bw_with("myitem/password", "bw://myitem/password", &runner).unwrap();
        assert_eq!(result.as_str(), "pw123");
    }

    #[test]
    fn fetch_bw_with_username_field() {
        let runner = MockSecretRunner::new().on_success(
            "bw",
            &["get", "username", "--", "myitem"],
            b"user1",
        );
        let result = fetch_bw_with("myitem/username", "bw://myitem/username", &runner).unwrap();
        assert_eq!(result.as_str(), "user1");
    }

    #[test]
    fn fetch_bw_with_no_field_defaults_to_password() {
        let runner =
            MockSecretRunner::new().on_success("bw", &["get", "password", "--", "myitem"], b"pw");
        let result = fetch_bw_with("myitem", "bw://myitem", &runner).unwrap();
        assert_eq!(result.as_str(), "pw");
    }

    #[test]
    fn fetch_bw_with_notes_field() {
        let runner = MockSecretRunner::new().on_success(
            "bw",
            &["get", "notes", "--", "myitem"],
            b"some notes",
        );
        let result = fetch_bw_with("myitem/notes", "bw://myitem/notes", &runner).unwrap();
        assert_eq!(result.as_str(), "some notes");
    }

    #[test]
    fn fetch_bw_with_uri_field() {
        let runner = MockSecretRunner::new().on_success(
            "bw",
            &["get", "uri", "--", "myitem"],
            b"https://example.com",
        );
        let result = fetch_bw_with("myitem/uri", "bw://myitem/uri", &runner).unwrap();
        assert_eq!(result.as_str(), "https://example.com");
    }
}
