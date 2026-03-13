# dotf

Your dotfiles belong in git. Your secrets don't.

dotf splits them: templates go in git, secret values stay in your password manager. On any machine, `dotf sync` fetches the secrets, renders the templates, and symlinks the real files into place.

```sh
brew tap chrisfentiman/dotf && brew install dotf
```

---

## The problem

You built a good shell setup. You want to version it, share it, clone it in 30 seconds on a new laptop. So you push it to GitHub — then you grep your configs and find your email in `.gitconfig`, an API token in `.npmrc`, a registry credential in `.cargo/config.toml`. The repo has to stay private, or the secrets have to come out.

The usual workaround is a `.localrc` or `.bash_profile_priv`: a file you source but never commit. It works on one machine. On a new machine you spend two hours with your old laptop open next to it, manually copying values across. Nothing documents what secrets are needed or where they came from.

dotf fixes this by making the secrets part of the repo — not their values, their *locations*. Every secret becomes a placeholder that maps to a URI in your password manager. At sync time, dotf fetches and injects them. Git only ever sees the template.

---

## How it works

You have a `.gitconfig` with your email in it. You want the file in git. You don't want your email in git.

```
# ~/dotfiles/configs/.gitconfig.tmpl  ← committed to git
[user]
  name  = Chris Fentiman
  email = {{GIT_EMAIL}}

[github]
  token = {{GITHUB_TOKEN}}
```

```toml
# ~/dotfiles/.secrets.toml  ← committed to git
[secrets]
GIT_EMAIL    = "op://personal/github/email"
GITHUB_TOKEN = "op://personal/github/token"
```

When you run `dotf sync`, it fetches `op://personal/github/email` from 1Password, renders the template, and writes the real file to `~/dotfiles/configs/.gitconfig` (gitignored). `~/.gitconfig` is a symlink to that rendered file.

The secret never touches git. The mapping does — so on a new machine, `dotf init` knows exactly what to fetch.

---

## Install

```sh
brew tap chrisfentiman/dotf
brew install dotf
```

Or with cargo:

```sh
cargo install dotf
```

---

## Quick start

```sh
# New machine — clone your dotfiles repo and render everything
dotf init

# Add a config file to be managed
dotf config ~/.gitconfig
# dotf shows the file, you identify the secret values,
# dotf replaces them with {{PLACEHOLDERS}} and asks for the URI

# Render all templates and sync to git
dotf sync
```

---

## Secret backends

dotf routes secrets by URI scheme. Use whichever password manager you already have. You can mix backends in the same `.secrets.toml`.

| URI | Password manager | CLI |
|---|---|---|
| `pass://vault/item/field` | Proton Pass | `brew install protonpass/pass/pass` |
| `op://vault/item/field` | 1Password | `brew install 1password-cli` |
| `bw://item-name/field` | Bitwarden | `brew install bitwarden-cli` |
| `env://VAR_NAME` | Environment variable | — |

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

## File layout

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
