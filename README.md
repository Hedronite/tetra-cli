<p align="center">
  <a href="https://github.com/VirtualMachinist/tetra-cli">
    <img src="brand/logo.jpg" alt="Tetra" width="280">
  </a>
</p>

<h1 align="center">Tetra</h1>

<p align="center">
  <strong>The world command of the Hedronite platform.</strong><br>
  Nickel is the language. <code>tetractl</code> is the verb. Facet, HedronDB, and h3s stay engines.
</p>

<p align="center">
  <a href="https://crates.io/crates/tetractl"><img src="https://img.shields.io/crates/v/tetractl?style=flat&colorA=0A0A0A&colorB=1A9B9B" alt="tetractl on crates.io"></a>
  <a href="https://rustup.rs"><img src="https://img.shields.io/badge/Rust-1.95-F46623?style=flat&colorA=0A0A0A&logo=rust&logoColor=white" alt="Rust 1.95"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-1A9B9B?style=flat&colorA=0A0A0A" alt="MIT OR Apache-2.0 license"></a>
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#what-you-get">What you get</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="#status">Status</a> ·
  <a href="#contributing">Contributing</a>
</p>

<p align="center">
  Built by <a href="https://hedronite.com">Hedronite</a>'s <a href="https://github.com/VirtualMachinist">VirtualMachinist</a>.
</p>

---

**Tetra** hosts the platform's only Nickel VM through `hedron-ncl`. Operators and agents type `tetractl`, not `facet ncl` + `hedron` HQL + `kubectl`. `check`, `eval`, `pack`, and `gen` run in-process. `apply` evaluates once and executes a frozen bundle through Facet. HedronDB still records intent. h3s + kubectl still own Ready. Tetra also fills `doctor`, `session`, `status`, `diff`, and `blame`.

```
tetra      — the binary
tetractl   — the kubectl-shaped alias of that same binary
```

It is the Nickel VM host, not a kubectl clone, and not an MCP server (yet).

| | Pin |
|---|---|
| Binary | `tetractl` / `tetra` ([`tetractl`](https://crates.io/crates/tetractl) 0.0.1) |
| Nickel host | this repo, `crates/hedron-ncl` (`k8s-1.34-h3s-0.9.1`) |
| Lattice history | [`facet-lattice`](https://crates.io/crates/facet-lattice) — different axis; there is no `facet-ncl` |
| MSRV | 1.95 |
| License | tetractl [Apache-2.0](LICENSE); hedron-ncl MIT OR Apache-2.0 ([LICENSE-MIT](LICENSE-MIT)) |

## Quick start

Rust **1.95**. `check` / `eval` / `pack` / `gen` run in-process. `facet` must be on `PATH` (or `FACET_BIN`) for `apply` / `session` / observe. `hedron` and `kubectl` are optional until you ask for intent or cluster.

```sh
cargo install tetractl
# from a clone: cargo run --bin tetractl --
```

```text
tetractl doctor
tetractl session start
tetractl check worlds/prod.ncl
tetractl eval  worlds/prod.ncl --json
tetractl apply worlds/prod.ncl --json
tetractl status worlds/prod.ncl
tetractl diff   worlds/prod.ncl
tetractl blame  '<Status JSON or kubectl stderr>'
```

`TETRA_SESSION` is a Facet session ULID (`FACET_SESSION`). `--json` emits Facet JSON (`schemaVersion: 1`). Extra flags after the world path (`--var`, `--import-path`, …) are parsed in-process. `apply` evaluates once, then execs Facet `apply --frozen -`.

crates.io name `tetra` is a game framework. Install **`tetractl`**. Both bins come from that crate.

## What you get

| You type | Engine |
|---|---|
| `tetractl check` / `eval` / `pack` / `gen` | in-process `hedron-ncl` |
| `tetractl apply` | in-process eval, then Facet `apply --frozen -` |
| intent status of names | HedronDB HQL |
| cluster get / Ready | `kubectl` on the host that has kubeconfig |
| `doctor` `session` `status` `diff` `blame` | Tetra only |

`facet ncl` remains the debug CLI. Hash and exit parity with `facet ncl check|export|pack` is the contract. `tetractl apply` matches `facet ncl apply` on stdout, stderr, exit, and the Lattice `ncl:apply` row.

### What you do not get

| Claim | Reality |
|---|---|
| A second Nickel VM | `Cargo.lock` has exactly one `nickel-lang-core` and one `hedron-ncl`; zero `rusqlite` |
| `tetra` exit 0 ⇒ Pod Ready | Ready is `kubectl get -o json` on the kubeconfig host |
| `tetra mcp` | Not shipped |
| Tokens in `.ncl` | Forbidden. Runtime ids stay out of desired state |
| `cargo install tetra` | Wrong crate |

## How it works

One rule: **Nickel is the world language; Tetra hosts the VM; apply freezes a bundle for Facet.**

- Desired state lives in `worlds/<vault>.ncl` (`{ cluster, intent, calls } \| World`).
- `tetractl check|eval|pack|gen` run in-process. `tetractl apply` evaluates once and execs Facet `apply --frozen -` (`FACET_BIN`, default `facet`).
- `tetractl session start|end` wraps `facet session`. The ULID is the join key.
- `tetractl status` reads the three engines by **names**. `diff` is desired export vs that status (exit 1 on drift). `blame` parses admission errors and never evals Nickel.
- `tetractl doctor` is capability + host law. It never prints secrets.

```
crates/tetractl/src/   # bins tetractl + tetra, clap, wrap apply
  ncl.rs eval.rs pack.rs gen.rs
  doctor.rs session.rs status.rs diff.rs blame.rs
crates/hedron-ncl/     # the Nickel VM (MIT OR Apache-2.0)
```

## Status

Workspace **0.0.1** matches crates.io [`tetractl`](https://crates.io/crates/tetractl). M1 verbs plus in-process `check` / `eval` / `pack` / `gen` are in this tree. CI is `.github/workflows/rust-tops.yml`.

## Contributing

Issues and pull requests are welcome. Start with [AGENTS.md](AGENTS.md). One Nickel VM via `hedron-ncl`, no tokens in Nickel, no invented query language.

```sh
cargo test --locked
cargo clippy --all-targets -- -D warnings
```

Rust **1.95**.

## Credits and license

Tetra is built and maintained by [Hedronite](https://hedronite.com). tetractl is Apache-2.0 ([LICENSE](LICENSE)); hedron-ncl is MIT OR Apache-2.0 ([LICENSE-MIT](LICENSE-MIT)). The Tetra name is Hedronite's.
