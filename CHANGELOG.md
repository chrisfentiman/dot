# Changelog

## [0.6.1](https://github.com/chrisfentiman/dot/compare/v0.6.0...v0.6.1) (2026-03-13)


### Bug Fixes

* release workflow uses correct binary name (dotf not dot) ([20261a2](https://github.com/chrisfentiman/dot/commit/20261a2691fe495c72425587369241eefb3d82c9))

## [0.6.0](https://github.com/chrisfentiman/dot/compare/v0.5.0...v0.6.0) (2026-03-13)


### Features

* add --dir flag for project-local dotfiles, replace Handlebars with single-pass renderer ([bed95b0](https://github.com/chrisfentiman/dot/commit/bed95b0e6d307d3df60ebb27c8adba428acd5ca7))
* rename binary dot→dotf, add similar diff, zeroize secrets, Password prompt, dynamic status widths ([f5fcf4c](https://github.com/chrisfentiman/dot/commit/f5fcf4c680ec65655a3a94709e637764725f2c44))
* wire Runner trait into sync, init, and modify commands ([e035011](https://github.com/chrisfentiman/dot/commit/e035011c31ca3598272dddeb88e6be006d46f735))


### Bug Fixes

* atomic writes for secrets/symlinks toml, hard-fail on secret fetch errors, validate symlink destinations ([e39f5f3](https://github.com/chrisfentiman/dot/commit/e39f5f3bc5a11359ed4037806273339b1bff99c9))
* **dotfiles:** harden template rendering, TOML parsing, and atomic writes ([2b6911c](https://github.com/chrisfentiman/dot/commit/2b6911c469b10c79e780791fdd547f53d0013460))
* **modify:** check VISUAL before EDITOR per Unix convention ([79d652c](https://github.com/chrisfentiman/dot/commit/79d652c2038c55ce063074a7f60eff11dc6c2d1f))
* **remove:** check config existence before empty symlinks guard ([afd83d5](https://github.com/chrisfentiman/dot/commit/afd83d51e1cd10dcc85037b2e459648894949d4c))
* **secrets:** validate placeholder names and URI schemes before storage ([09f28bd](https://github.com/chrisfentiman/dot/commit/09f28bdb2920241a1e41ed2e6e476cafdb4b5efa))
* **status:** detect dangling symlinks as broken instead of wrong target ([11013ae](https://github.com/chrisfentiman/dot/commit/11013ae4c159847746dadf5887a920557e661d51))
* **sync:** harden git workflow and improve commit logic ([61e8ac7](https://github.com/chrisfentiman/dot/commit/61e8ac70fc1836bbcccbc5a5c1f887c0a79c8aec))
* **sync:** split git add into tracked update and new file staging ([3824427](https://github.com/chrisfentiman/dot/commit/382442786100447fbb9beb6102e4c998863a6bbd))
* **tests:** add env_lock() to all tests that mutate environment variables ([fda3019](https://github.com/chrisfentiman/dot/commit/fda3019b22b1c7ef06dab03b5f19557eae445289))

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
