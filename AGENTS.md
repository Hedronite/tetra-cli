# Agents

Tetra is the world command. You are talking to a *world*, not to three tools.

Tetra hosts the platform's only Nickel VM through `hedron-ncl`. `tetractl check`,
`eval`, `pack`, and `gen` run in-process. `apply` evaluates once and executes a
frozen bundle through Facet (`apply --frozen -`). Tetra does not open Lattice and does not link
`rusqlite`. `Cargo.lock` carries exactly one `nickel-lang-core` and exactly one
`hedron-ncl`.

`tetractl gen pod-profile` and `tetractl gen docs-eod` emit embedded plain-Rust
templates keyed to `hedron_ncl::CONTRACT_SET` (`k8s-1.34-h3s-0.9.1`). They are
not derived from the overlay at runtime.

## Do

- Author desired state in Nickel (`worlds/<vault>.ncl`, `intent/<vault>/<name>.intent.ncl`).
- Enter through `tetractl` (or `tetra --json` / later `tetra mcp`).
- Stamp work with `TETRA_SESSION` (same ULID as `FACET_SESSION`).
- Treat `facet`, `hedron`, and `kubectl` as engines for apply, observe, and cluster.

## Do not

- Put tokens, kubeconfig, or vault tokens in `.ncl`.
- Add a second `nickel-lang-core` or `rusqlite` to this repo's lockfile.
- Reimplement `facet ncl apply` / Hedron `put` / h3s admission.
- Merge Lattice, the vault, and h3s Storage into one file.
- Invent a query language. Ask Lattice, HQL, or kubectl through tetractl.
- Call `facet mcp`, `hedron hql`, and `kubectl` in a loop and call that a platform session. Start `tetractl session` first.

## First tools

```text
tetractl doctor
tetractl check worlds/prod.ncl
tetractl eval  worlds/prod.ncl --json
tetractl pack  --out pack --json
tetractl gen   pod-profile --out pod_profile_gen.rs
tetractl diff  worlds/prod.ncl --json
tetractl apply worlds/prod.ncl --json
```
