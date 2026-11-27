//! `tetractl apply` freeze-and-handoff. HOME + FACET_CONFIG_DIR + FACET_DATA_DIR
//! live in the sandbox so the operator Lattice store is never opened.

#![allow(dead_code, unused_imports)]
mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::*;
use serde_json::{json, Value};

struct Sandbox {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    pack: PathBuf,
    world: PathBuf,
}

fn isolate(cmd: &mut Command, home: &Path) {
    hermetic(cmd, home);
    let data = home.join("facetdata");
    let _ = fs::create_dir_all(&data);
    cmd.env("FACET_DATA_DIR", &data);
}

fn tetra(args: &[&str], cwd: &Path, home: &Path) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tetractl"));
    isolate(&mut cmd, home);
    if let Some(bin) = std::env::var_os("FACET_BIN") {
        cmd.env("FACET_BIN", bin);
    }
    cmd.args(args)
        .current_dir(cwd)
        .output()
        .expect("tetractl runs")
}

fn facet_cmd(args: &[&str], cwd: &Path, home: &Path) -> Output {
    let bin = require_facet();
    let mut cmd = Command::new(bin);
    isolate(&mut cmd, home);
    cmd.args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|error| panic!("facet must be runnable ({error}); set FACET_BIN"))
}

fn sandbox() -> Sandbox {
    let _facet = require_facet();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    let pack = root.join("pack");
    let out = facet_cmd(
        &["ncl", "pack", "--out", pack.to_str().unwrap(), "--json"],
        &root,
        &home,
    );
    assert!(
        out.status.success(),
        "facet ncl pack: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let world = root.join("world.ncl");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/world.ncl"),
        &world,
    )
    .unwrap();
    let hist = facet_cmd(&["history", "--json"], &root, &home);
    assert_eq!(
        hist.status.code(),
        Some(0),
        "init lattice: {}",
        String::from_utf8_lossy(&hist.stderr)
    );
    Sandbox {
        _dir: dir,
        root,
        home,
        pack,
        world,
    }
}

fn blob_bytes(root: &Path, hash: &str) -> Vec<u8> {
    fs::read(root.join(".facet/blobs").join(hash)).unwrap_or_else(|error| {
        panic!("blob {hash}: {error}");
    })
}

fn apply_rows(sb: &Sandbox) -> Vec<Value> {
    let out = facet_cmd(&["history", "--json"], &sb.root, &sb.home);
    assert_eq!(out.status.code(), Some(0));
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    value["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["requestPath"] == "ncl:apply")
        .cloned()
        .collect()
}

#[test]
fn apply_json_human_quiet_match_facet_ncl_apply() {
    let sb = sandbox();
    let world = "world.ncl";
    let pack = sb.pack.to_str().unwrap();
    let t = tetra(
        &["--json", "apply", world, "--import-path", pack],
        &sb.root,
        &sb.home,
    );
    let f = facet_cmd(
        &["ncl", "apply", world, "--import-path", pack, "--json"],
        &sb.root,
        &sb.home,
    );
    assert_parity(&t, &f, &sb.root, "apply --json");
    let human_t = tetra(&["apply", world, "--import-path", pack], &sb.root, &sb.home);
    let human_f = facet_cmd(
        &["ncl", "apply", world, "--import-path", pack],
        &sb.root,
        &sb.home,
    );
    assert_parity(&human_t, &human_f, &sb.root, "apply human");
    let quiet_t = tetra(
        &["apply", world, "--import-path", pack, "-q"],
        &sb.root,
        &sb.home,
    );
    let quiet_f = facet_cmd(
        &["ncl", "apply", world, "--import-path", pack, "-q"],
        &sb.root,
        &sb.home,
    );
    assert_parity(&quiet_t, &quiet_f, &sb.root, "apply -q");
    assert!(quiet_t.stdout.is_empty());
}

#[test]
fn apply_var_and_relative_path_match_facet_ncl_apply() {
    let sb = sandbox();
    let pack = sb.pack.to_str().unwrap();
    let var = "calls.info.name=\"overridden\"";
    let t = tetra(
        &[
            "--json",
            "apply",
            "world.ncl",
            "--import-path",
            pack,
            "--var",
            var,
        ],
        &sb.root,
        &sb.home,
    );
    let f = facet_cmd(
        &[
            "ncl",
            "apply",
            "world.ncl",
            "--import-path",
            pack,
            "--var",
            var,
            "--json",
        ],
        &sb.root,
        &sb.home,
    );
    assert_parity(&t, &f, &sb.root, "apply --var");
    let t = tetra(
        &["--json", "apply", "./world.ncl", "--import-path", pack],
        &sb.root,
        &sb.home,
    );
    let f = facet_cmd(
        &[
            "ncl",
            "apply",
            "./world.ncl",
            "--import-path",
            pack,
            "--json",
        ],
        &sb.root,
        &sb.home,
    );
    assert_parity(&t, &f, &sb.root, "apply ./world.ncl");
    let v: Value = serde_json::from_slice(&t.stdout).unwrap();
    assert_eq!(v["path"], "./world.ncl");
}

#[test]
fn apply_session_forwarding_and_lattice_blob_parity() {
    let sb = sandbox();
    let pack = sb.pack.to_str().unwrap();
    let mut tetra_cmd = Command::new(env!("CARGO_BIN_EXE_tetractl"));
    isolate(&mut tetra_cmd, &sb.home);
    if let Some(bin) = std::env::var_os("FACET_BIN") {
        tetra_cmd.env("FACET_BIN", bin);
    }
    tetra_cmd
        .env("TETRA_SESSION", "01ARZ3NDEKTSV4RRFFQ69G5FAV")
        .env_remove("FACET_SESSION")
        .args(["--json", "apply", "world.ncl", "--import-path", pack])
        .current_dir(&sb.root);
    let t = tetra_cmd.output().unwrap();
    assert_eq!(
        t.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&t.stderr)
    );
    let mut facet_ncl = Command::new(require_facet());
    isolate(&mut facet_ncl, &sb.home);
    facet_ncl
        .env("FACET_SESSION", "01ARZ3NDEKTSV4RRFFQ69G5FAV")
        .args(["ncl", "apply", "world.ncl", "--import-path", pack, "--json"])
        .current_dir(&sb.root);
    let f = facet_ncl.output().unwrap();
    assert_parity(&t, &f, &sb.root, "apply session json");
    let rows = apply_rows(&sb);
    assert!(rows.len() >= 2);
    let frozen = &rows[0];
    let ncl = &rows[1];
    assert_eq!(frozen["method"], "NCL");
    assert_eq!(frozen["requestPath"], "ncl:apply");
    assert_eq!(frozen["sessionId"], "01ARZ3NDEKTSV4RRFFQ69G5FAV");
    assert_eq!(frozen["method"], ncl["method"]);
    assert_eq!(frozen["requestPath"], ncl["requestPath"]);
    assert_eq!(frozen["requestHash"], ncl["requestHash"]);
    assert_eq!(frozen["url"], ncl["url"]);
    assert_eq!(frozen["tags"], ncl["tags"]);
    assert_eq!(frozen["varNames"], ncl["varNames"]);
    assert_eq!(frozen["sessionId"], ncl["sessionId"]);
    let req_a = frozen["request"]["body"]["hash"].as_str().unwrap();
    let req_b = ncl["request"]["body"]["hash"].as_str().unwrap();
    let res_a = frozen["response"]["body"]["hash"].as_str().unwrap();
    let res_b = ncl["response"]["body"]["hash"].as_str().unwrap();
    assert_eq!(req_a, req_b);
    assert_eq!(res_a, res_b);
    assert_eq!(blob_bytes(&sb.root, req_a), blob_bytes(&sb.root, req_b));
    assert_eq!(blob_bytes(&sb.root, res_a), blob_bytes(&sb.root, res_b));
}

#[test]
fn apply_facet_session_wins_over_tetra_session() {
    let sb = sandbox();
    let pack = sb.pack.to_str().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tetractl"));
    isolate(&mut cmd, &sb.home);
    if let Some(bin) = std::env::var_os("FACET_BIN") {
        cmd.env("FACET_BIN", bin);
    }
    cmd.env("TETRA_SESSION", "01TETRASESSIONULID000000000")
        .env("FACET_SESSION", "01FACETSESSIONULID000000000")
        .args(["--json", "apply", "world.ncl", "--import-path", pack])
        .current_dir(&sb.root);
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let rows = apply_rows(&sb);
    assert_eq!(rows[0]["sessionId"], "01FACETSESSIONULID000000000");
}

#[test]
fn apply_intent_put_when_hedron_db_set() {
    let sb = sandbox();
    let pack = sb.pack.to_str().unwrap();
    let db = sb.root.join("hedron.db");
    let mut tcmd = Command::new(env!("CARGO_BIN_EXE_tetractl"));
    isolate(&mut tcmd, &sb.home);
    if let Some(bin) = std::env::var_os("FACET_BIN") {
        tcmd.env("FACET_BIN", bin);
    }
    tcmd.env("FACET_HEDRON_DB", &db)
        .args(["--json", "apply", "world.ncl", "--import-path", pack])
        .current_dir(&sb.root);
    let t = tcmd.output().unwrap();
    assert_eq!(
        t.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&t.stderr)
    );
    let v: Value = serde_json::from_slice(&t.stdout).unwrap();
    assert_eq!(v["action"]["intent"]["skipped"], false);
    assert_eq!(v["action"]["intent"]["put"], 1);
}

fn spy_bin(root: &Path) -> PathBuf {
    let spy = root.join("spy-facet");
    let marker = root.join("spy-ran");
    let stdin_path = root.join("spy-stdin.json");
    let argv_path = root.join("spy-argv.txt");
    fs::write(
        &spy,
        format!(
            "#!/bin/sh\n: > '{marker}'\ncat > '{stdin}'\nprintf '%s\\n' \"$@\" > '{argv}'\nprintf '%s\\n' '{{\"schemaVersion\":1,\"path\":\"world.ncl\",\"ok\":true}}'\n",
            marker = marker.display(),
            stdin = stdin_path.display(),
            argv = argv_path.display(),
        ),
    )
    .unwrap();
    let mut perms = fs::metadata(&spy).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&spy, perms).unwrap();
    spy
}

#[test]
fn apply_sends_ten_key_bundle_once_on_stdin() {
    let sb = sandbox();
    let pack = sb.pack.to_str().unwrap();
    let spy = spy_bin(&sb.root);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tetractl"));
    isolate(&mut cmd, &sb.home);
    cmd.env("FACET_BIN", &spy)
        .args([
            "--json",
            "apply",
            "world.ncl",
            "--import-path",
            pack,
            "--var",
            "calls.info.name=\"overridden\"",
        ])
        .current_dir(&sb.root);
    let out = cmd.output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(sb.root.join("spy-ran").is_file());
    let argv = fs::read_to_string(sb.root.join("spy-argv.txt")).unwrap();
    let line = argv.lines().collect::<Vec<_>>();
    assert_eq!(line[0], "apply");
    assert_eq!(line[1], "--frozen");
    assert_eq!(line[2], "-");
    assert!(line.contains(&"--json"));
    let bundle: Value = serde_json::from_slice(&fs::read(sb.root.join("spy-stdin.json")).unwrap())
        .expect("bundle JSON");
    let obj = bundle.as_object().unwrap();
    assert_eq!(obj.len(), 10);
    for key in [
        "schemaVersion",
        "path",
        "moduleHash",
        "exportHash",
        "contractSet",
        "varNames",
        "sourceSnapshot",
        "cluster",
        "intent",
        "calls",
    ] {
        assert!(obj.contains_key(key), "{key}");
    }
    assert_eq!(bundle["schemaVersion"], 1);
    assert_eq!(bundle["path"], "world.ncl");
    assert_eq!(bundle["varNames"], json!(["calls.info.name"]));
    assert_eq!(bundle["sourceSnapshot"]["entry"], "world.ncl");
    assert!(bundle["sourceSnapshot"]["sources"]["world.ncl"].is_string());
    assert_eq!(bundle["moduleHash"].as_str().unwrap().len(), 64);
}

#[test]
fn apply_bad_nickel_does_not_spawn_facet() {
    let sb = sandbox();
    fs::write(sb.root.join("bad.ncl"), "let x = \n").unwrap();
    let spy = spy_bin(&sb.root);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tetractl"));
    isolate(&mut cmd, &sb.home);
    cmd.env("FACET_BIN", &spy)
        .args(["--json", "apply", "bad.ncl"])
        .current_dir(&sb.root);
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(!sb.root.join("spy-ran").exists());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["category"], "ncl_invalid");
    assert_eq!(v["error"]["exitCode"], 5);
}

#[test]
fn apply_missing_facet_fails_loudly() {
    let sb = sandbox();
    let pack = sb.pack.to_str().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tetractl"));
    isolate(&mut cmd, &sb.home);
    cmd.env("FACET_BIN", "/nonexistent/facet")
        .args(["--json", "apply", "world.ncl", "--import-path", pack])
        .current_dir(&sb.root);
    let out = cmd.output().unwrap();
    assert_ne!(out.status.code(), Some(0));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("/nonexistent/facet"), "{err}");
}

#[test]
fn apply_rejects_non_schema_version_one() {
    let sb = sandbox();
    let pack = sb.pack.to_str().unwrap();
    let spy = sb.root.join("bad-schema");
    fs::write(
        &spy,
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"ok\":true}'\n",
    )
    .unwrap();
    let mut perms = fs::metadata(&spy).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&spy, perms).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tetractl"));
    isolate(&mut cmd, &sb.home);
    cmd.env("FACET_BIN", &spy)
        .args(["--json", "apply", "world.ncl", "--import-path", pack])
        .current_dir(&sb.root);
    let out = cmd.output().unwrap();
    assert_ne!(out.status.code(), Some(0));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("schemaVersion"), "{err}");
}

#[test]
fn apply_refuses_frozen_with_v1_text() {
    let sb = sandbox();
    let pack = sb.pack.to_str().unwrap();
    let out = tetra(
        &[
            "--json",
            "apply",
            "world.ncl",
            "--import-path",
            pack,
            "--frozen",
        ],
        &sb.root,
        &sb.home,
    );
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["category"], "invalid_arguments");
    assert_eq!(
        v["error"]["message"],
        "ncl apply does not support --frozen; use ncl export --frozen"
    );
}
