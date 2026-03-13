# Security Policy

## Supported Versions

Only the latest release is supported with security fixes.

## Reporting Vulnerabilities

If you find a security issue, please report it privately via [GitHub's security advisory feature](https://github.com/chrisfentiman/dot/security/advisories/new). Do not open a public issue.

## Threat Model

`dotf` manages dotfiles that may contain secrets (API keys, tokens, credentials, email addresses). The security design centers on one principle: **the git remote is untrusted; secrets must never be committed.**

### Trust boundaries

| Zone | Trust level | Examples |
|------|-------------|---------|
| **Local machine** | Trusted | Filesystem, process memory, user session |
| **Password manager CLIs** | Trusted | `pass`, `op`, `bw` -- assumed to return correct values |
| **Git remote** | Untrusted | `~/dotfiles` repo on GitHub -- only templates and URI mappings, never secret values |
| **Rendered files** | Sensitive | Written to disk with restricted permissions, never committed |

### What dotf protects against

**Secret leakage to git.** Templates use `{{PLACEHOLDER}}` syntax. Rendered outputs (containing real values) are gitignored and never staged. Only templates and `.secrets.toml` (which maps placeholders to password manager URIs, not actual values) are committed. The `dotf sync` command explicitly stages only known safe paths (`configs/*.tmpl`, `.symlinks.toml`, `.secrets.toml`).

**Secrets in rendered files on disk.** Rendered config files are written with `0o600` permissions (owner read/write only). The write target directory is validated to not be world-writable.

**Subprocess environment leakage.** All child processes (`pass`, `op`, `bw`, `git`, `brew`) are spawned with `env_clear()` and an explicit allowlist of safe environment variables:
- General: `HOME`, `PATH`, `USER`, `LOGNAME`, `SHELL`, `TERM`, `LANG`, `LC_*`, `EDITOR`, `VISUAL`, `XDG_*`, `TMPDIR`, `TZ`
- Git: `GIT_AUTHOR_*`, `GIT_COMMITTER_*`, `SSH_AUTH_SOCK`, `GPG_TTY`, `GNUPGHOME`
- Homebrew: `HOMEBREW_*`
- 1Password: `OP_SESSION_*`, `OP_SERVICE_ACCOUNT_TOKEN` (validated: alphanumeric + underscore, max 40 chars)

No ambient secrets (e.g., `AWS_SECRET_ACCESS_KEY`, `DATABASE_URL`) leak to password manager or git subprocesses.

**Secret memory residency.** Fetched secret values are held in `Zeroizing<String>` from the `zeroize` crate. When the value goes out of scope, the memory is overwritten with zeros before deallocation. Error messages from failed secret fetches are truncated to 512 bytes and zeroized after display.

**Template injection.** The template renderer uses a single-pass substitution algorithm. After replacing `{{PLACEHOLDER}}` with a secret value, the renderer does not re-scan the output for new placeholders. A secret value containing `{{OTHER_SECRET}}` is treated as literal text. This prevents a compromised password manager entry from exfiltrating other secrets.

**Path traversal.** Symlink destinations are canonicalized and verified to be inside the root directory (`$HOME` in global mode, the project root in `--dir` mode). Path components containing `..` are rejected. In local mode, absolute paths and tilde (`~`) paths in symlink targets are rejected entirely.

**Symlink clobbering.** `ensure_symlink` refuses to overwrite regular files. Only existing symlinks are replaced, and replacement is atomic (create temp symlink, then rename). This prevents a malicious `.symlinks.toml` entry from overwriting arbitrary files.

**Atomic writes.** All file writes (rendered configs, `.secrets.toml`, `.symlinks.toml`) use a tempfile-then-rename pattern. The tempfile is created in the same directory as the target, permissions are set before the rename, and the parent directory is fsynced after. This prevents partial writes from crashes, disk-full conditions, or interrupted processes.

**TOML injection.** Both `SecretsFile` and `SymlinksFile` use `#[serde(deny_unknown_fields)]` to reject unexpected keys during deserialization. Placeholder names are validated to be non-empty ASCII alphanumeric plus underscore. URI schemes are validated against the known set (`pass://`, `op://`, `bw://`, `env://`).

### What dotf does NOT protect against

**Compromised password manager.** If `pass`, `op`, or `bw` return malicious values, dotf will render them into your config files. dotf trusts the password manager CLI to authenticate properly and return correct data.

**Local root access.** A root user or process with elevated privileges can read rendered files, process memory, or swap. This is outside dotf's threat model -- if an attacker has root on your machine, dotf (and your password manager) are already compromised.

**Rendered file content in memory.** After template rendering, the output string exists in process memory until written to disk and dropped. This output string is *not* zeroized (only the individual fetched secret values are). The window is small (milliseconds during a typical sync) but the rendered content could theoretically be extracted from a core dump taken during that window.

**Swap and core dumps.** Zeroized memory may still appear in swap files or core dumps if the OS pages out the memory before `drop()` runs. Mitigations:
- Use encrypted swap (default on macOS, configurable on Linux)
- Disable core dumps: `ulimit -c 0`
- On Linux: `echo 0 > /proc/sys/kernel/core_pattern`

**Side-channel attacks.** dotf does not implement constant-time comparison or other side-channel mitigations. This is appropriate for a local CLI tool that never handles authentication or key exchange directly.

**Denial of service via `.secrets.toml`.** A malicious `.secrets.toml` (e.g., from a compromised git remote) could reference URIs that cause password manager CLIs to hang or prompt repeatedly. dotf does not currently impose timeouts on subprocess execution.
