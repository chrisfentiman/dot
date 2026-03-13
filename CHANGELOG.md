# Changelog

## [0.4.0] - 2026-03-13

### Added
- Pluggable secret backends: `pass://` (Proton Pass), `op://` (1Password), `bw://` (Bitwarden), `env://` (environment variable)
- `dotf secrets list` now shows a BACKEND column

### Changed
- `dotf init` no longer hardcodes Proton Pass setup — detects which backends are needed from `.secrets.toml` and checks their CLIs
- All user-facing strings scrubbed of Proton Pass specifics

## [0.3.1] - 2026-03-13

### Fixed
- Release workflow: add `-L` to curl to follow GitHub redirects (was producing empty-file SHA)
- `Cargo.toml` version auto-set from git tag in release CI

## [0.3.0] - 2026-03-10

### Added
- `dotf remove [name]` — stop managing a config, with optional file restore
- `dotf diff [name]` — preview what a sync would change, in memory
- `dotf sync` now uses `--rebase` and reports merge conflicts with resolution instructions

### Fixed
- `dotf config` now derives the correct symlink target from the file's actual path (handles `~/.config/...` subdirectories)

## [0.2.5] - 2026-03-10

### Fixed
- Release workflow: replace heredoc with `printf` to fix YAML parsing failure
- Formula auto-update pipeline fully working end-to-end

## [0.2.0] - 2026-03-09

### Added
- Initial release
- `dotf init`, `dotf config`, `dotf modify`, `dotf sync`, `dotf status`
- `dotf secrets list/add/remove/validate`
- `dotf completions` for zsh/bash/fish
- Handlebars template rendering with Proton Pass secret injection
- Homebrew formula via `chrisfentiman/hometaps`
