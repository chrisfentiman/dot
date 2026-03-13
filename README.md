# dotf

Your dotfiles belong in git. Your secrets don't.

`dotf` is a single Rust binary that manages dotfiles with template rendering and pluggable secret injection. Templates go in git, secret values stay in your password manager. On any machine, `dotf sync` fetches the secrets, renders the templates, and symlinks the real files into place.

```sh
brew tap chrisfentiman/dotf && brew install dotf
```

## The problem

You built a good shell setup. You want to version it, share it, clone it on a new laptop in 30 seconds. So you push it to GitHub -- then you grep your configs and find your email in `.gitconfig`, an API token in `.npmrc`, a registry credential in `.cargo/config.toml`.

The usual fix is a `.localrc` or `.bash_profile_priv` -- a file you source but never commit. It works on one machine. On a new machine you spend an hour with your old laptop open next to it, manually copying values. Nothing documents what secrets are needed or where they came from.

Existing dotfiles tools solve one piece but not the whole problem:

| Tool | Templates | Secrets | Symlinks | The catch |
|------|:---------:|:-------:|:--------:|-----------|
| **GNU Stow** | -- | -- | Symlink farm | No templating or secrets. Machine-specific configs require external scripts. |
| **yadm** | Minimal | Git-crypt (whole-file) | -- | Alternate files are full copies per machine, not variable substitution. No runtime secret injection from password managers. |
| **dotbot** | -- | -- | YAML-driven | Just a symlink + shell runner. Requires Python runtime. |
| **rcm** | -- | -- | Tag-based | No templates, no secrets, no encryption. Unix only, low activity. |
| **chezmoi** | Go `text/template` | GPG/age + PM integrations | -- (copies) | Most powerful option, but secrets are embedded in template syntax: `{{ (bitwarden "item").password }}`. Steep learning curve with source/target state model and filename-prefix attributes. |
| **home-manager** | Nix expressions | agenix/sops-nix | Nix-managed | Requires learning Nix. Overkill if you just need config files with a few secrets. |
| **dotf** | `{{PLACEHOLDER}}` | Declarative `.secrets.toml` | `.symlinks.toml` | -- |

dotf fixes this by making the secrets part of the repo -- not their *values*, their *locations*. Every secret becomes a placeholder that maps to a URI in your password manager. At sync time, dotf fetches and injects them. Git only ever sees the template.

## How it works

You have a `.gitconfig` with your email in it. You want the file in git. You don't want your email in git.

```
# ~/dotfiles/configs/.gitconfig.tmpl  <-- committed to git
[user]
  name  = Chris Fentiman
  email = {{GIT_EMAIL}}

[github]
  token = {{GITHUB_TOKEN}}
```

```toml
# ~/dotfiles/.secrets.toml  <-- committed to git (URIs only, never values)
[secrets]
GIT_EMAIL    = "op://personal/github/email"
GITHUB_TOKEN = "op://personal/github/token"
```

When you run `dotf sync`:

1. Fetches `op://personal/github/email` from 1Password
2. Renders the template with the real values
3. Writes `~/dotfiles/configs/.gitconfig` (gitignored, `0o600` permissions)
4. Symlinks `~/.gitconfig` to the rendered file
5. Commits and pushes the dotfiles repo

The secret never touches git. The mapping does -- so on a new machine, `dotf init` knows exactly what to fetch.

## Install

### Homebrew (macOS and Linux)

```sh
brew tap chrisfentiman/dotf
brew install dotf
```

### From source

```sh
cargo install --git https://github.com/chrisfentiman/dot.git
```

### Pre-built binaries

Download from the [releases page](https://github.com/chrisfentiman/dot/releases). Each release includes binaries for macOS (ARM/x86) and Linux (ARM/x86) with SHA256 checksums.

## Quick start

```sh
# New machine -- clone your dotfiles repo and render everything
dotf init

# Add a config file to be managed
dotf config ~/.gitconfig
# dotf shows the file, you mark the secret values,
# it replaces them with {{PLACEHOLDERS}} and asks for the URI

# Check what's managed and what's broken
dotf status

# Render all templates and sync to git
dotf sync
```

## Secret backends

dotf routes secrets by URI scheme. Use whichever password manager you already have. You can mix backends in the same `.secrets.toml`.

| URI scheme | Password manager | CLI |
|---|---|---|
| `pass://vault/item/field` | Proton Pass | [`pass`](https://proton.me/pass) |
| `op://vault/item/field` | 1Password | [`op`](https://1password.com/downloads/command-line) |
| `bw://item-name/field` | Bitwarden | [`bw`](https://bitwarden.com/help/cli/) |
| `env://VAR_NAME` | Environment variable | -- |

Backends are pluggable -- adding a new one is a single match arm in `src/secret.rs`.

## Commands

| Command | Description |
|---|---|
| `dotf init` | Clone dotfiles repo, check CLIs, install completions, render all templates |
| `dotf config <path>` | Add a config file -- interactively extract secrets into `{{PLACEHOLDERS}}` |
| `dotf modify [name]` | Edit a template in `$EDITOR`, re-render on save |
| `dotf sync` | `git pull --rebase`, render all templates, commit and push |
| `dotf diff [name]` | Preview what sync would change, without writing anything |
| `dotf status` | Health check -- which configs are ok, missing, or broken |
| `dotf remove [name]` | Stop managing a config, optionally restore the file in place |
| `dotf secrets list` | Show all placeholder-to-URI mappings with backend column |
| `dotf secrets validate` | Test that every secret can actually be fetched |
| `dotf secrets add <n> <uri>` | Add a secret mapping |
| `dotf secrets remove <name>` | Remove a secret mapping |
| `dotf completions <shell>` | Print shell completions (bash, zsh, fish) |

### Project-local mode

Use `--dir <path>` to manage project-scoped dotfiles (`.env`, `.claude/settings.json`, etc.):

```sh
dotf --dir . init          # creates .dotf/ in current directory
dotf --dir . config .env   # template + secrets for project .env
dotf --dir . sync          # render only, no git operations
```

## File layout

```
~/dotfiles/
  configs/
    .gitconfig.tmpl     <-- template, committed
    .gitconfig          <-- rendered output, gitignored
    .zshrc.tmpl
    .zshrc
  .secrets.toml         <-- placeholder -> URI map, committed
  .symlinks.toml        <-- name -> target path map, committed
  .gitignore            <-- ignores rendered outputs
  Brewfile              <-- optional, run by dotf init
```

`~/.gitconfig` is a symlink to `~/dotfiles/configs/.gitconfig`, which is rendered from `.gitconfig.tmpl` at sync time.

## Security

See [SECURITY.md](SECURITY.md) for the full threat model. Key properties:

- **Secrets never enter git.** Templates use `{{PLACEHOLDER}}` syntax. Rendered outputs are gitignored.
- **Rendered files are `0o600`.** Owner read/write only.
- **Subprocess isolation.** All child processes run with `env_clear()` and an explicit allowlist. No ambient secrets leak.
- **Memory safety.** Secret values use `Zeroizing<String>` and are zeroed on drop.
- **Path traversal protection.** Symlink targets are canonicalized and verified inside `$HOME`. Paths with `..` are rejected.
- **Atomic writes.** All file writes use tempfile-then-rename. No partial writes on crash.
- **TOML injection prevention.** Config types use `#[serde(deny_unknown_fields)]`.

## Why dotf over chezmoi?

chezmoi is the most feature-complete dotfiles manager available. If you need OS-conditional logic, run-once scripts, or external file fetching, use chezmoi.

dotf is for the common case: config files with some secrets that you want in git without the secrets leaking.

| | chezmoi | dotf |
|---|---|---|
| **Template syntax** | Go `text/template` + Sprig: `{{ if eq .chezmoi.os "darwin" }}` | `{{PLACEHOLDER}}` -- nothing to learn |
| **Secrets in templates** | Embedded: `{{ (bitwarden "item").password }}` | Separated: template says `{{DB_PASS}}`, `.secrets.toml` maps it to `bw://db/password` |
| **Switch password managers** | Edit every template that references the old backend | Change one line in `.secrets.toml` |
| **File management** | Copies files from source to target | Symlinks to rendered files |
| **Secret auditing** | Manual | `dotf secrets list` and `dotf secrets validate` |
| **Concepts to learn** | Source state, target state, filename attributes (`dot_`, `private_`, `run_once_`, `modify_`) | Two TOML files and `{{PLACEHOLDER}}` syntax |
| **Runtime** | Go binary | Rust binary |

## Contributing

Issues and PRs welcome at [github.com/chrisfentiman/dot](https://github.com/chrisfentiman/dot).

```sh
cargo test           # run tests
cargo clippy         # lint
cargo fmt --check    # formatting
```

## License

[MIT](LICENSE)
