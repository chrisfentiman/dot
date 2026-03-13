# dotf

**Dotfiles manager with pluggable secret injection.**

Your configs live in git as templates. Secrets stay in your password manager. `dotf` fetches them at sync time and renders the real files — gitignored — then symlinks them into place.

```
[user]
  email = {{GIT_EMAIL}}   ← template, safe to commit
```
```toml
# .secrets.toml
GIT_EMAIL = "op://personal/github/email"
```

📖 **[Documentation](https://chrisfentiman.github.io/dot/)**

---

## Install

```sh
brew tap chrisfentiman/hometaps
brew install dotf
```

## Quick start

```sh
# First time on a new machine
dotf init

# Add a config file to manage
dotf config ~/.gitconfig

# Fetch secrets, render, sync to git
dotf sync
```

## Secret backends

| URI scheme | Password manager |
|---|---|
| `pass://vault/item/field` | Proton Pass |
| `op://vault/item/field` | 1Password |
| `bw://item-name/field` | Bitwarden |
| `env://VAR_NAME` | Environment variable |

## Commands

| Command | Description |
|---|---|
| `dotf init` | Bootstrap a new machine — clone repo, check CLIs, render all |
| `dotf config <path>` | Add a config file, interactively extract secrets into placeholders |
| `dotf modify [name]` | Open a template in `$EDITOR`, re-renders on save |
| `dotf sync` | Pull → render → push |
| `dotf diff [name]` | Preview what a sync would change |
| `dotf status` | Show health of all managed configs |
| `dotf remove [name]` | Stop managing a config, optionally restore the file |
| `dotf secrets list\|add\|remove\|validate` | Manage secret placeholder mappings |
| `dotf completions <shell>` | Generate shell completions (bash, zsh, fish) |

## How it works

```
~/.gitconfig.tmpl   ←  committed to git (no secrets)
        ↓  dotf sync fetches secrets from your password manager
~/dotfiles/configs/.gitconfig   ←  rendered output (gitignored)
        ↓  symlinked
~/.gitconfig   ←  what your tools actually read
```

## License

MIT
