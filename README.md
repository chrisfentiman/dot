# dotf

dotf manages your dotfiles as templates. Secrets become `{{PLACEHOLDERS}}` that map to your password manager — dotf fetches and injects them at sync time. Your git repo gets the templates. Your password manager keeps the values.

```sh
brew tap chrisfentiman/hometaps && brew install dotf
```

---

## How it works

You have a `.gitconfig` with your email in it. You want it in git. You don't want your email in git.

dotf solves this by splitting the file in two:

```
# ~/dotfiles/configs/.gitconfig.tmpl  ← committed
[user]
  name  = Chris Fentiman
  email = {{GIT_EMAIL}}
```

```toml
# ~/dotfiles/.secrets.toml  ← committed
[secrets]
GIT_EMAIL = "op://personal/github/email"
```

When you run `dotf sync`, it fetches `op://personal/github/email` from 1Password, renders the template, and writes the real file to `~/dotfiles/configs/.gitconfig` (gitignored). `~/.gitconfig` is a symlink to that rendered file.

The secret never touches git.

---

## Install

```sh
brew tap chrisfentiman/hometaps
brew install dotf
```

Or with cargo:

```sh
cargo install dotf
```

---

## Quick start

```sh
# First time — clone your dotfiles repo and render everything
dotf init

# Add a config file to be managed
dotf config ~/.gitconfig
# dotf shows the file, asks which values are secrets,
# replaces them with {{PLACEHOLDERS}}, writes the template

# Render all templates and sync to git
dotf sync
```

---

## Secret backends

dotf routes secrets by URI scheme. Use whichever password manager you already have.

| URI | Password manager | CLI |
|---|---|---|
| `pass://vault/item/field` | Proton Pass | `brew install protonpass/pass/pass` |
| `op://vault/item/field` | 1Password | `brew install 1password-cli` |
| `bw://item-name/field` | Bitwarden | `brew install bitwarden-cli` |
| `env://VAR_NAME` | Environment variable | — |

You can mix backends in the same `.secrets.toml`.

---

## Commands

```
dotf init                   Clone dotfiles repo, check CLIs, render all templates
dotf config <path>          Add a config file — interactively extract secrets
dotf modify [name]          Edit a template in $EDITOR, re-renders on save
dotf sync                   git pull --rebase → render → git push
dotf diff [name]            Show what a sync would change, without writing anything
dotf status                 Health check — which configs are ok, missing, or broken
dotf remove [name]          Stop managing a config, optionally restore the file
dotf secrets list           Show all placeholder → URI mappings with backend column
dotf secrets validate       Test that every secret can actually be fetched
dotf secrets add <n> <uri>  Add a secret mapping
dotf secrets remove <name>  Remove a secret mapping
dotf completions <shell>    Print shell completions (bash, zsh, fish)
```

---

## The file layout

```
~/dotfiles/
  configs/
    .gitconfig.tmpl     ← template, committed to git
    .gitconfig          ← rendered output, gitignored
    .zshrc.tmpl
    .zshrc
  .secrets.toml         ← placeholder → URI map, committed
  .symlinks.toml        ← name → target path map, committed
  .gitignore            ← ignores rendered outputs
  Brewfile              ← optional, run by dotf init
```

`~/.gitconfig` → symlink → `~/dotfiles/configs/.gitconfig` → rendered from template at sync time.

---

## License

MIT
