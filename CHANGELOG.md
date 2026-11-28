# Changelog

Notable changes to the Tetra toolchain (`tetractl` and `hedron-ncl`).

## [0.1.0] - 2026-11-28

First real `tetractl` release on crates.io (0.0.1 was a placeholder).

### Added

- `tetractl` hosts the Nickel VM in `hedron-ncl`: `check`, `eval`, `pack`, `gen`, `apply`, and `doctor`.
- `apply` evaluates once, freezes a bundle, and hands off to `FACET_BIN apply --frozen -` (requires Facet **0.9.0** or newer).

