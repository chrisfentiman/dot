# Changelog

## [0.7.0](https://github.com/chrisfentiman/dot/compare/v0.6.1...v0.7.0) (2026-03-13)


### Features

* add --dir flag for project-local dotfiles, replace Handlebars with single-pass renderer ([bed95b0](https://github.com/chrisfentiman/dot/commit/bed95b0e6d307d3df60ebb27c8adba428acd5ca7))
* add remove, diff commands; fix config symlink target; improve sync conflict handling ([c4c8dba](https://github.com/chrisfentiman/dot/commit/c4c8dba15bafdbd83414bf751e7e3f3602921be5))
* add shell completions command, install completions in dot init ([67c6847](https://github.com/chrisfentiman/dot/commit/67c6847d288799396325e325044a71a314d2be5d))
* cross-platform builds and public homebrew tap ([74ea3e0](https://github.com/chrisfentiman/dot/commit/74ea3e055df24ba34364cd3426e818c6206ffc0f))
* implement all dot commands ([e530627](https://github.com/chrisfentiman/dot/commit/e530627b73aa302b549e9da39f322070a0dcee49))
* pluggable secret backends (pass, op, bw, env); remove internal Proton Pass references ([ccb6baf](https://github.com/chrisfentiman/dot/commit/ccb6baf1102863a6889222af8eaabfdc1ec7ad7d))
* rename binary dot→dotf, add similar diff, zeroize secrets, Password prompt, dynamic status widths ([f5fcf4c](https://github.com/chrisfentiman/dot/commit/f5fcf4c680ec65655a3a94709e637764725f2c44))
* scaffold dot CLI — commands, release workflow, CI ([e5cfa79](https://github.com/chrisfentiman/dot/commit/e5cfa79f3d2655f246893117acb75ee65542132a))
* wire Runner trait into sync, init, and modify commands ([e035011](https://github.com/chrisfentiman/dot/commit/e035011c31ca3598272dddeb88e6be006d46f735))


### Bug Fixes

* add -L flag to curl in release workflow to follow GitHub redirects ([0827b7d](https://github.com/chrisfentiman/dot/commit/0827b7d8ca08df8426a2a07b39fe5c6e2764780d))
* atomic writes for secrets/symlinks toml, hard-fail on secret fetch errors, validate symlink destinations ([e39f5f3](https://github.com/chrisfentiman/dot/commit/e39f5f3bc5a11359ed4037806273339b1bff99c9))
* bump Cargo.toml version to 0.3.0; auto-set from tag in release CI ([9abee29](https://github.com/chrisfentiman/dot/commit/9abee2996f8d8440b61bb16f640b907fc03d9e87))
* cross-compile x86_64 from ARM runner instead of macos-13 ([266abb1](https://github.com/chrisfentiman/dot/commit/266abb137796b6cdf4198c930ffc6666a2dc754c))
* **dotfiles:** harden template rendering, TOML parsing, and atomic writes ([2b6911c](https://github.com/chrisfentiman/dot/commit/2b6911c469b10c79e780791fdd547f53d0013460))
* **modify:** check VISUAL before EDITOR per Unix convention ([79d652c](https://github.com/chrisfentiman/dot/commit/79d652c2038c55ce063074a7f60eff11dc6c2d1f))
* release workflow uses correct binary name (dotf not dot) ([20261a2](https://github.com/chrisfentiman/dot/commit/20261a2691fe495c72425587369241eefb3d82c9))
* **remove:** check config existence before empty symlinks guard ([afd83d5](https://github.com/chrisfentiman/dot/commit/afd83d51e1cd10dcc85037b2e459648894949d4c))
* rename binary to dotf to avoid conflict with graphviz dot ([9486e69](https://github.com/chrisfentiman/dot/commit/9486e695658a8352fc5029b5525dc507bdc03a73))
* replace heredoc with printf in release workflow to fix YAML parsing ([71f6e78](https://github.com/chrisfentiman/dot/commit/71f6e788e5243ef25469b39dee4218875e286ecb))
* retry asset download in update-formula job until available ([a83e7ad](https://github.com/chrisfentiman/dot/commit/a83e7ad0ac9fdbd32487aca8f8ef3347de805584))
* **secrets:** validate placeholder names and URI schemes before storage ([09f28bd](https://github.com/chrisfentiman/dot/commit/09f28bdb2920241a1e41ed2e6e476cafdb4b5efa))
* sha256sum with shasum fallback for cross-platform compat ([#9](https://github.com/chrisfentiman/dot/issues/9)) ([6a41f00](https://github.com/chrisfentiman/dot/commit/6a41f00540a6f45f8373513135ab0de39a5f8ba8))
* **status:** detect dangling symlinks as broken instead of wrong target ([11013ae](https://github.com/chrisfentiman/dot/commit/11013ae4c159847746dadf5887a920557e661d51))
* **sync:** harden git workflow and improve commit logic ([61e8ac7](https://github.com/chrisfentiman/dot/commit/61e8ac70fc1836bbcccbc5a5c1f887c0a79c8aec))
* **sync:** split git add into tracked update and new file staging ([3824427](https://github.com/chrisfentiman/dot/commit/382442786100447fbb9beb6102e4c998863a6bbd))
* **tests:** add env_lock() to all tests that mutate environment variables ([fda3019](https://github.com/chrisfentiman/dot/commit/fda3019b22b1c7ef06dab03b5f19557eae445289))
* use is_err() instead of !is_ok() to satisfy clippy ([c8e3848](https://github.com/chrisfentiman/dot/commit/c8e384810a0147151f925c9c4ce221d41df11258))
* use macos-latest for both macOS targets (macos-13 deprecated) ([2d54d00](https://github.com/chrisfentiman/dot/commit/2d54d00bc7414e39d1e5938f1bb419074525bee6))

## [0.6.1](https://github.com/chrisfentiman/dot/compare/v0.6.0...v0.6.1) (2026-03-13)


### Bug Fixes

* sha256sum with shasum fallback for cross-platform compat ([#9](https://github.com/chrisfentiman/dot/issues/9)) ([6a41f00](https://github.com/chrisfentiman/dot/commit/6a41f00540a6f45f8373513135ab0de39a5f8ba8))

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
