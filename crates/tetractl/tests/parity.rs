//! T2 G3 migration parity: tetractl vs facet ncl check|export|pack.
//!
//! Fail closed when FACET_BIN is missing (never skip). Live comparisons run
//! in a hermetic HOME / FACET_CONFIG_DIR and drop FACET_NO_RECORD unless a
//! case sets it. stdout/stderr are compared after substituting the workspace
//! root with `<W>` and Nickel closure addresses `%<closure@0x…>` with
//! `%<closure@ADDR>` (QA T2 G1 N4).
//!
//! D4 clap-owned cases **exempt** from Facet byte parity (clap, not the ncl
//! parser, owns them):
//! - `--json` twice as a clap global bool (`tetractl --json --json …`)
//! - bare / unknown subcommand (`tetractl`, `tetractl recon`)
//! - `--version`
//!
//! No-path `check`/`eval` is **not** clap-owned since G1: it reaches the
//! in-process parser and is compared. Every other clap-pass-through flag
//! (`--var`, `--import-path`, `-q`, `--frozen`, unknown options) is compared
//! byte-for-byte.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{assert_parity, hermetic, require_facet, tetractl};

struct World {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

impl World {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(root.join("facetcfg")).unwrap();
        fs::create_dir_all(root.join("tetra")).unwrap();
        fs::create_dir_all(root.join("worlds")).unwrap();
        fs::create_dir_all(root.join("emptydir")).unwrap();
        fs::write(root.join("plainfile"), "not a dir\n").unwrap();
        fs::write(root.join("scalar.ncl"), "[1, 2, 3]\n").unwrap();
        fs::write(root.join("syntax.ncl"), "{ a = }\n").unwrap();
        fs::write(
            root.join("bad_contract.ncl"),
            "{ replicas | Number = \"three\" }\n",
        )
        .unwrap();
        fs::write(
            root.join("secret.ncl"),
            "{ cluster = [], calls = { token | not_exported = \"hunter2\", info = { name = \"x\" } } }\n",
        )
        .unwrap();
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        copy_tree(&manifest.join("../hedron-ncl/ncl"), &root.join("ncl"));
        fs::copy(
            manifest.join("tests/fixtures/world.ncl"),
            root.join("tetra/world.ncl"),
        )
        .unwrap();
        fs::copy(
            manifest.join("tests/fixtures/world.ncl"),
            root.join("worlds/prod.ncl"),
        )
        .unwrap();
        let pack = tetractl(&["pack", "--out", "pack", "--json"], &root, &home);
        assert_eq!(
            pack.status.code(),
            Some(0),
            "in-process pack: {}{}",
            String::from_utf8_lossy(&pack.stdout),
            String::from_utf8_lossy(&pack.stderr)
        );
        copy_tree(&root.join("pack"), &root.join("pack_drift"));
        let plat = root.join("pack_drift/hedron-ncl/platform.ncl");
        let mut p = fs::read(&plat).unwrap();
        p.extend_from_slice(b"\n# drift\n");
        fs::write(&plat, p).unwrap();
        World {
            _dir: dir,
            root,
            home,
        }
    }
}

fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), to).unwrap();
        }
    }
}

fn tetra_env(w: &World, args: &[&str], extra: &[(&str, Option<&str>)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tetractl"));
    hermetic(&mut cmd, &w.home);
    if let Some(bin) = std::env::var_os("FACET_BIN") {
        cmd.env("FACET_BIN", bin);
    }
    for (k, v) in extra {
        match v {
            Some(val) => {
                cmd.env(k, val);
            }
            None => {
                cmd.env_remove(k);
            }
        }
    }
    cmd.args(args)
        .current_dir(&w.root)
        .output()
        .expect("tetractl")
}

fn facet_env(w: &World, args: &[&str], extra: &[(&str, Option<&str>)]) -> Output {
    let bin = require_facet();
    let mut cmd = Command::new(bin);
    hermetic(&mut cmd, &w.home);
    for (k, v) in extra {
        match v {
            Some(val) => {
                cmd.env(k, val);
            }
            None => {
                cmd.env_remove(k);
            }
        }
    }
    cmd.args(args)
        .current_dir(&w.root)
        .output()
        .unwrap_or_else(|e| panic!("facet ({e})"))
}

fn cmp(w: &World, name: &str, tetra: &[&str], facet_args: &[&str], extra: &[(&str, Option<&str>)]) {
    let t = tetra_env(w, tetra, extra);
    let f = facet_env(w, facet_args, extra);
    assert_parity(&t, &f, &w.root, name);
}

#[test]
fn facet_bin_missing_fails_closed() {
    if std::env::var_os("FACET_BIN").is_none() {
        let _ = require_facet();
    } else {
        require_facet();
    }
}

#[test]
fn live_check_eval_pack_parity_battery() {
    let _facet = require_facet();
    let w = World::new();
    let pack = "pack";

    cmp(
        &w,
        "check tetra/world --json",
        &["check", "tetra/world.ncl", "--import-path", pack, "--json"],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "eval tetra/world --json",
        &["eval", "tetra/world.ncl", "--import-path", pack, "--json"],
        &[
            "ncl",
            "export",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "check fixture world",
        &[
            "check",
            "ncl/fixtures/world.ncl",
            "--import-path",
            pack,
            "--json",
        ],
        &[
            "ncl",
            "check",
            "ncl/fixtures/world.ncl",
            "--import-path",
            pack,
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "eval fixture world",
        &[
            "eval",
            "ncl/fixtures/world.ncl",
            "--import-path",
            pack,
            "--json",
        ],
        &[
            "ncl",
            "export",
            "ncl/fixtures/world.ncl",
            "--import-path",
            pack,
            "--json",
        ],
        &[],
    );
    let t = tetra_env(
        &w,
        &[
            "check",
            "ncl/fixtures/world_cluster_only.ncl",
            "--import-path",
            pack,
            "--json",
        ],
        &[],
    );
    let f = facet_env(
        &w,
        &[
            "ncl",
            "check",
            "ncl/fixtures/world_cluster_only.ncl",
            "--import-path",
            pack,
            "--json",
        ],
        &[],
    );
    assert_eq!(t.status.code(), Some(5), "cluster_only is negative");
    assert_parity(&t, &f, &w.root, "world_cluster_only");

    cmp(
        &w,
        "check overlay",
        &["check", "ncl/overlay/k8s-1.34-h3s-0.9.1.ncl", "--json"],
        &[
            "ncl",
            "check",
            "ncl/overlay/k8s-1.34-h3s-0.9.1.ncl",
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "eval overlay",
        &["eval", "ncl/overlay/k8s-1.34-h3s-0.9.1.ncl", "--json"],
        &[
            "ncl",
            "export",
            "ncl/overlay/k8s-1.34-h3s-0.9.1.ncl",
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "check platform",
        &["check", "ncl/platform.ncl", "--json"],
        &["ncl", "check", "ncl/platform.ncl", "--json"],
        &[],
    );
    cmp(
        &w,
        "eval platform",
        &["eval", "ncl/platform.ncl", "--json"],
        &["ncl", "export", "ncl/platform.ncl", "--json"],
        &[],
    );

    let var = "calls.info.name=\"overridden\"";
    cmp(
        &w,
        "eval --var",
        &[
            "eval",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            var,
            "--json",
        ],
        &[
            "ncl",
            "export",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            var,
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "eval repeated --var",
        &[
            "eval",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "calls.info.name=\"a\"",
            "--var",
            "calls.info.name=\"b\"",
            "--json",
        ],
        &[
            "ncl",
            "export",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "calls.info.name=\"a\"",
            "--var",
            "calls.info.name=\"b\"",
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "eval unknown-path --var",
        &[
            "eval",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "no.such.path=\"x\"",
            "--json",
        ],
        &[
            "ncl",
            "export",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "no.such.path=\"x\"",
            "--json",
        ],
        &[],
    );

    let t = tetra_env(
        &w,
        &["eval", "secret.ncl", "--import-path", pack, "--json"],
        &[],
    );
    let f = facet_env(
        &w,
        &[
            "ncl",
            "export",
            "secret.ncl",
            "--import-path",
            pack,
            "--json",
        ],
        &[],
    );
    assert_parity(&t, &f, &w.root, "secret eval");
    let blob = [&t.stdout[..], &t.stderr[..]].concat();
    assert!(
        !blob.windows(7).any(|w| w == b"hunter2"),
        "not_exported token must not leak"
    );

    let t = tetra_env(&w, &["check", "scalar.ncl", "--json"], &[]);
    let f = facet_env(&w, &["ncl", "check", "scalar.ncl", "--json"], &[]);
    assert_eq!(t.status.code(), Some(0), "check scalar");
    assert_parity(&t, &f, &w.root, "check scalar");
    let t = tetra_env(&w, &["eval", "scalar.ncl", "--json"], &[]);
    let f = facet_env(&w, &["ncl", "export", "scalar.ncl", "--json"], &[]);
    assert_eq!(t.status.code(), Some(5), "eval scalar");
    assert_parity(&t, &f, &w.root, "eval scalar");

    cmp(
        &w,
        "missing file json",
        &["--json", "check", "nope.ncl"],
        &["ncl", "check", "nope.ncl", "--json"],
        &[],
    );
    cmp(
        &w,
        "syntax json",
        &["check", "syntax.ncl", "--json"],
        &["ncl", "check", "syntax.ncl", "--json"],
        &[],
    );
    cmp(
        &w,
        "contract blame json",
        &["check", "bad_contract.ncl", "--json"],
        &["ncl", "check", "bad_contract.ncl", "--json"],
        &[],
    );
    cmp(
        &w,
        "bad --var noeq",
        &[
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "noeq",
            "--json",
        ],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "noeq",
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "secret --var",
        &[
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "calls.token=\"x\"",
            "--json",
        ],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "calls.token=\"x\"",
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "secret --var API_KEY",
        &[
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "calls.API_KEY=1",
            "--json",
        ],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "calls.API_KEY=1",
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "import drift",
        &[
            "check",
            "tetra/world.ncl",
            "--import-path",
            "pack_drift",
            "--json",
        ],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            "pack_drift",
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "import not dir",
        &[
            "check",
            "tetra/world.ncl",
            "--import-path",
            "plainfile",
            "--json",
        ],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            "plainfile",
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "import repeat",
        &[
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--import-path",
            pack,
            "--json",
        ],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--import-path",
            pack,
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "empty import suppresses env",
        &["check", "tetra/world.ncl", "--import-path", "", "--json"],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            "",
            "--json",
        ],
        &[("FACET_NCL_IMPORT_PATH", Some(pack))],
    );
    cmp(
        &w,
        "json+quiet",
        &[
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--json",
            "-q",
        ],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--json",
            "-q",
        ],
        &[],
    );
    cmp(
        &w,
        "check --frozen",
        &[
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--frozen",
            "--json",
        ],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--frozen",
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "unknown option",
        &[
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--nope",
            "--json",
        ],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--nope",
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "check -q",
        &["check", "tetra/world.ncl", "--import-path", pack, "-q"],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "-q",
        ],
        &[],
    );

    cmp(
        &w,
        "pack json",
        &["pack", "--out", "out_json", "--json"],
        &["ncl", "pack", "--out", "out_json", "--json"],
        &[],
    );
    cmp(
        &w,
        "pack human",
        &["pack", "--out", "out_human"],
        &["ncl", "pack", "--out", "out_human"],
        &[],
    );
    cmp(
        &w,
        "pack trailing slash",
        &["pack", "--out", "out_slash/"],
        &["ncl", "pack", "--out", "out_slash/"],
        &[],
    );
    cmp(
        &w,
        "pack missing --out",
        &["pack", "--json"],
        &["ncl", "pack", "--json"],
        &[],
    );
    cmp(
        &w,
        "pack extra positional",
        &["pack", "extra", "--out", "out_pos", "--json"],
        &["ncl", "pack", "extra", "--out", "out_pos", "--json"],
        &[],
    );
    cmp(
        &w,
        "pack --out twice",
        &["pack", "--out", "a", "--out", "b", "--json"],
        &["ncl", "pack", "--out", "a", "--out", "b", "--json"],
        &[],
    );
    cmp(
        &w,
        "pack --out no value",
        &["pack", "--out"],
        &["ncl", "pack", "--out"],
        &[],
    );
    cmp(
        &w,
        "pack unwritable",
        &["pack", "--out", "plainfile/sub", "--json"],
        &["ncl", "pack", "--out", "plainfile/sub", "--json"],
        &[],
    );

    let _ = facet_env(&w, &["ncl", "pack", "--out", "out_facet"], &[]);
    let _ = tetra_env(&w, &["pack", "--out", "out_tetra"], &[]);
    assert_eq!(
        collect_rel(&w.root.join("out_tetra")),
        collect_rel(&w.root.join("out_facet")),
        "written pack trees"
    );
}

fn collect_rel(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    fn walk(dir: &Path, prefix: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        if !dir.exists() {
            return;
        }
        let mut ents: Vec<_> = fs::read_dir(dir).unwrap().map(|e| e.unwrap()).collect();
        ents.sort_by_key(|e| e.file_name());
        for e in ents {
            let p = e.path();
            let rel = p
                .strip_prefix(prefix)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if p.is_dir() {
                walk(&p, prefix, out);
            } else {
                out.push((rel, fs::read(&p).unwrap()));
            }
        }
    }
    walk(root, root, &mut out);
    out
}

#[test]
fn offline_check_eval_pack_gen_doctor_without_facet() {
    let w = World::new();
    let home = &w.home;
    let gone = "/nonexistent/facet";
    let run = |args: &[&str]| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_tetractl"));
        hermetic(&mut cmd, home);
        cmd.env("FACET_BIN", gone)
            .args(args)
            .current_dir(&w.root)
            .output()
            .unwrap()
    };
    let check = run(&[
        "check",
        "tetra/world.ncl",
        "--import-path",
        "pack",
        "--json",
    ]);
    assert_eq!(check.status.code(), Some(0), "offline check");
    let eval = run(&["eval", "tetra/world.ncl", "--import-path", "pack", "--json"]);
    assert_eq!(eval.status.code(), Some(0), "offline eval");
    let pack = run(&["pack", "--out", "offline_pack", "--json"]);
    assert_eq!(pack.status.code(), Some(0), "offline pack");
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let pod = run(&["gen", "pod-profile"]);
    assert_eq!(pod.status.code(), Some(0), "offline gen pod-profile");
    let pod_src = fs::read(manifest.join("templates/pod_profile_gen.rs")).unwrap();
    assert_eq!(pod.stdout, pod_src, "pod-profile bytes");
    assert_eq!(
        json_sha(&run(&["gen", "pod-profile", "--json"])),
        "5c34d855b5012428350702e0b650de89f89879db3cda1fd9eb2abe5a01b828ad"
    );
    let docs = run(&["gen", "docs-eod"]);
    assert_eq!(docs.status.code(), Some(0), "offline gen docs-eod");
    let docs_src = fs::read(manifest.join("templates/docs_eod.rs")).unwrap();
    assert_eq!(docs.stdout, docs_src, "docs-eod bytes");
    assert_eq!(
        json_sha(&run(&["gen", "docs-eod", "--json"])),
        "e013d039590950065a0006aaf8868e3eee97af154e59aad1ab8c67cda5b9b33d"
    );
    for src in [&pod.stdout[..], &docs.stdout[..]] {
        let text = String::from_utf8_lossy(src);
        assert!(!text.contains("use nickel_lang_core"));
        assert!(!text.contains("hedron_ncl::"));
    }
    let doctor = run(&["doctor", "--json"]);
    assert_eq!(
        doctor.status.code(),
        Some(5),
        "doctor without Facet is exit 5 (apply/session still need it): {}{}",
        String::from_utf8_lossy(&doctor.stdout),
        String::from_utf8_lossy(&doctor.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&doctor.stdout).expect("doctor json");
    assert_eq!(v["capabilities"]["ncl"], true);
    assert_eq!(v["capabilities"]["pack"], true);
    assert!(
        w.root
            .join(".tetra/pack/k8s-1.34-h3s-0.9.1/hedron-ncl/platform.ncl")
            .is_file(),
        "doctor materializes pack without FACET_BIN"
    );
}

fn json_sha(out: &Output) -> String {
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("gen --json");
    v["sha256"].as_str().unwrap().to_owned()
}

#[test]
fn replace_closures_normalizes_nickel_addresses() {
    assert_eq!(
        common::replace_closures("err %<closure@0xabc123> more %<closure@0x0> end"),
        "err %<closure@ADDR> more %<closure@ADDR> end"
    );
    assert_eq!(common::replace_closures("no closures"), "no closures");
}

#[test]
fn live_g1_blockers_human_eval_and_forms() {
    let _facet = require_facet();
    let w = World::new();
    let pack = "pack";

    // G1-B3: parse-loop / positional / --var / --frozen order.
    cmp(
        &w,
        "B3 check --frozen no path",
        &["check", "--frozen", "--json"],
        &["ncl", "check", "--frozen", "--json"],
        &[],
    );
    cmp(
        &w,
        "B3 eval --frozen no path",
        &["eval", "--frozen", "--json"],
        &["ncl", "export", "--frozen", "--json"],
        &[],
    );
    cmp(
        &w,
        "B3 eval --frozen twice",
        &[
            "eval",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--frozen",
            "--frozen",
            "--json",
        ],
        &[
            "ncl",
            "export",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--frozen",
            "--frozen",
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "B3 --var noeq --nope",
        &["check", "--var", "noeq", "--nope", "--json"],
        &["ncl", "check", "--var", "noeq", "--nope", "--json"],
        &[],
    );
    cmp(
        &w,
        "B3 secret --var then noeq",
        &[
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "calls.token=1",
            "--var",
            "noeq",
            "--json",
        ],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "calls.token=1",
            "--var",
            "noeq",
            "--json",
        ],
        &[],
    );
    cmp(
        &w,
        "B3 --var noeq no path",
        &["check", "--var", "noeq", "--json"],
        &["ncl", "check", "--var", "noeq", "--json"],
        &[],
    );
    cmp(
        &w,
        "B3 import-path twice no path",
        &[
            "check",
            "--import-path",
            pack,
            "--import-path",
            pack,
            "--json",
        ],
        &[
            "ncl",
            "check",
            "--import-path",
            pack,
            "--import-path",
            pack,
            "--json",
        ],
        &[],
    );

    // G1-B4: unreadable pack -> materialized-pack prefix.
    let unread = w.root.join("pack_unreadable");
    copy_tree(&w.root.join("pack"), &unread);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&unread).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&unread, perms).unwrap();
    }
    cmp(
        &w,
        "B4 unreadable pack",
        &[
            "check",
            "tetra/world.ncl",
            "--import-path",
            "pack_unreadable",
            "--json",
        ],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            "pack_unreadable",
            "--json",
        ],
        &[],
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&unread).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&unread, perms).unwrap();
    }

    // Human eval (kills M8).
    cmp(
        &w,
        "eval human",
        &["eval", "tetra/world.ncl", "--import-path", pack],
        &["ncl", "export", "tetra/world.ncl", "--import-path", pack],
        &[],
    );
    cmp(
        &w,
        "eval human --var",
        &[
            "eval",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "calls.info.name=\"overridden\"",
        ],
        &[
            "ncl",
            "export",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--var",
            "calls.info.name=\"overridden\"",
        ],
        &[],
    );

    // Closure addresses (G1 N4).
    cmp(
        &w,
        "eval scalar --var x=1 (closures)",
        &["eval", "scalar.ncl", "--var", "x=1", "--json"],
        &["ncl", "export", "scalar.ncl", "--var", "x=1", "--json"],
        &[],
    );

    // eval --frozen with a path.
    cmp(
        &w,
        "eval --frozen with path",
        &[
            "eval",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--frozen",
            "--json",
        ],
        &[
            "ncl",
            "export",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--frozen",
            "--json",
        ],
        &[],
    );

    // TETRA_NCL_IMPORT_PATH is Tetra-only and wins over FACET_NCL_IMPORT_PATH (D2).
    let t = tetra_env(
        &w,
        &["check", "tetra/world.ncl", "--json"],
        &[
            ("TETRA_NCL_IMPORT_PATH", Some(pack)),
            ("FACET_NCL_IMPORT_PATH", Some("pack_drift")),
        ],
    );
    assert_eq!(
        t.status.code(),
        Some(0),
        "TETRA import path must beat FACET_NCL_IMPORT_PATH: {}{}",
        String::from_utf8_lossy(&t.stdout),
        String::from_utf8_lossy(&t.stderr)
    );
    // CLI --import-path still beats TETRA (and is comparable to facet).
    cmp(
        &w,
        "CLI import-path beats TETRA env",
        &["check", "tetra/world.ncl", "--import-path", pack, "--json"],
        &[
            "ncl",
            "check",
            "tetra/world.ncl",
            "--import-path",
            pack,
            "--json",
        ],
        &[("TETRA_NCL_IMPORT_PATH", Some("pack_drift"))],
    );

    // --var= / --import-path= forms.
    cmp(
        &w,
        "equals-form flags",
        &[
            "eval",
            "tetra/world.ncl",
            "--import-path=pack",
            "--var=calls.info.name=\"eq\"",
            "--json",
        ],
        &[
            "ncl",
            "export",
            "tetra/world.ncl",
            "--import-path=pack",
            "--var=calls.info.name=\"eq\"",
            "--json",
        ],
        &[],
    );

    // Absolute path.
    let abs = w.root.join("tetra/world.ncl");
    let abs_s = abs.to_str().unwrap();
    cmp(
        &w,
        "absolute path eval",
        &["eval", abs_s, "--import-path", pack, "--json"],
        &["ncl", "export", abs_s, "--import-path", pack, "--json"],
        &[],
    );

    // No-path (not clap-owned since G1).
    cmp(
        &w,
        "check no path",
        &["check", "--json"],
        &["ncl", "check", "--json"],
        &[],
    );
    cmp(
        &w,
        "eval no path",
        &["eval", "--json"],
        &["ncl", "export", "--json"],
        &[],
    );
}
