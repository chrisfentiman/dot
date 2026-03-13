# Security Policy

## Threat Model

`dotf` manages dotfiles that may contain secrets (API keys, email addresses, tokens). The security boundary is:

**Trusted:** The local machine, the user's password manager CLIs (`pass`, `op`, `bw`), and the local filesystem.

**Untrusted:** The git remote (dotfiles repo). Secrets must never be committed.

### What dotf protects against

- **Secret leakage to git**: Templates use `{{PLACEHOLDER}}` syntax. Rendered outputs (which contain real values) are gitignored. Only templates and `.secrets.toml` (which maps placeholders to password manager URIs, not actual values) are committed.
- **Secrets in rendered files**: Rendered config files are written with `0o600` permissions (owner read/write only).
- **Subprocess environment leakage**: All child processes (`pass`, `op`, `bw`, `git`, `brew`) run with `env_clear()` and an explicit allowlist of safe environment variables. No ambient secrets leak to subprocesses.
- **Secret memory residency**: Secret values are held in `Zeroizing<String>` (from the `zeroize` crate) and zeroed on drop. The rendered output string is the only non-zeroized copy and lives only as long as needed for the file write.
- **Path traversal**: Symlink destinations are canonicalized and verified to be inside `$HOME`. Filenames containing `..` are rejected.
- **Symlink clobbering**: `ensure_symlink` refuses to overwrite regular files — only existing symlinks are replaced (atomically via rename).
- **Atomic writes**: Config files and TOML state files are written via tempfile-then-rename to prevent partial writes on crash or disk-full.
- **TOML injection**: Both `SecretsFile` and `SymlinksFile` use `#[serde(deny_unknown_fields)]` to reject unexpected keys.

### What dotf does NOT protect against

- **Compromised password manager**: If `pass`, `op`, or `bw` are compromised, dotf will fetch and render whatever they return.
- **Local root access**: A root user can read rendered files, memory, or swap. This is outside dotf's threat model.
- **Rendered file content in memory**: After rendering, the output string exists in process memory until written and dropped. It is not zeroized (only the fetched secret values are).
- **Swap/core dumps**: Zeroized memory may still appear in swap or core dumps. Use OS-level protections (encrypted swap, `ulimit -c 0`) if this matters.

## Reporting Vulnerabilities

If you find a security issue, please report it privately via GitHub's security advisory feature on the repository, or email the maintainer directly. Do not open a public issue.

## Supported Versions

Only the latest release is supported with security fixes.
