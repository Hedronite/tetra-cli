//! Hermetic Facet environment for integration tests (QA T2 G2 N6).
//! Every spawn that talks to Facet gets an isolated HOME + FACET_CONFIG_DIR
//! and drops FACET_NO_RECORD / import-path env unless the test sets them.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn require_facet() -> PathBuf {
    let bin = std::env::var_os("FACET_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("facet"));
    match Command::new(&bin).arg("--version").output() {
        Ok(out) if out.status.success() => bin,
        Ok(out) => panic!(
            "FACET_BIN={bin:?} is not runnable (exit {:?}); parity/wrap tests fail closed, never skip: {}{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(err) => panic!(
            "FACET_BIN missing or not runnable ({err}); set FACET_BIN to a real facet. Tests never skip."
        ),
    }
}

pub fn hermetic(cmd: &mut Command, home: &Path) {
    let cfg = home.join("facetcfg");
    let _ = std::fs::create_dir_all(home);
    let _ = std::fs::create_dir_all(&cfg);
    cmd.env_remove("FACET_NO_RECORD");
    cmd.env_remove("FACET_NCL_IMPORT_PATH");
    cmd.env_remove("TETRA_NCL_IMPORT_PATH");
    cmd.env_remove("FACET_SESSION");
    cmd.env_remove("TETRA_SESSION");
    cmd.env_remove("FACET_ACTOR");
    cmd.env("HOME", home);
    cmd.env("FACET_CONFIG_DIR", &cfg);
    cmd.env("NO_COLOR", "1");
    cmd.env("LC_ALL", "C");
    cmd.env("TZ", "UTC");
}

pub fn tetractl(args: &[&str], cwd: &Path, home: &Path) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tetractl"));
    hermetic(&mut cmd, home);
    if let Some(bin) = std::env::var_os("FACET_BIN") {
        cmd.env("FACET_BIN", bin);
    }
    cmd.args(args)
        .current_dir(cwd)
        .output()
        .expect("tetractl runs")
}

pub fn tetra_bin(bin: &str, args: &[&str], cwd: &Path, home: &Path) -> Output {
    let exe = match bin {
        "tetra" => env!("CARGO_BIN_EXE_tetra"),
        _ => env!("CARGO_BIN_EXE_tetractl"),
    };
    let mut cmd = Command::new(exe);
    hermetic(&mut cmd, home);
    if let Some(facet) = std::env::var_os("FACET_BIN") {
        cmd.env("FACET_BIN", facet);
    }
    cmd.args(args)
        .current_dir(cwd)
        .output()
        .expect("tetra bin runs")
}

pub fn facet(args: &[&str], cwd: &Path, home: &Path) -> Output {
    let bin = require_facet();
    let mut cmd = Command::new(bin);
    hermetic(&mut cmd, home);
    cmd.env(
        "FACET_BIN",
        std::env::var_os("FACET_BIN").unwrap_or_else(|| "facet".into()),
    );
    cmd.args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("facet must be runnable ({e}); set FACET_BIN"))
}

pub fn normalize(bytes: &[u8], root: &Path) -> Vec<u8> {
    let mut s = String::from_utf8_lossy(bytes).into_owned();
    s = s.replace(&root.display().to_string(), "<W>");
    if let Ok(canon) = root.canonicalize() {
        s = s.replace(&canon.display().to_string(), "<W>");
    }
    replace_closures(&s).into_bytes()
}

fn replace_closures(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("%<closure@0x") {
        out.push_str(&rest[..i]);
        rest = &rest[i + "%<closure@0x".len()..];
        let hex_end = rest
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(rest.len());
        if rest[hex_end..].starts_with('>') {
            out.push_str("%<closure@ADDR>");
            rest = &rest[hex_end + 1..];
        } else {
            out.push_str("%<closure@0x");
        }
    }
    out.push_str(rest);
    out
}

pub fn assert_parity(tetra: &Output, facet: &Output, root: &Path, what: &str) {
    let t_out = normalize(&tetra.stdout, root);
    let f_out = normalize(&facet.stdout, root);
    let t_err = normalize(&tetra.stderr, root);
    let f_err = normalize(&facet.stderr, root);
    assert_eq!(
        tetra.status.code(),
        facet.status.code(),
        "{what}: exit tetra={:?} facet={:?}\n tetra err {}\n facet err {}",
        tetra.status.code(),
        facet.status.code(),
        String::from_utf8_lossy(&t_err),
        String::from_utf8_lossy(&f_err)
    );
    assert_eq!(
        t_out,
        f_out,
        "{what}: stdout\n tetra {}\n facet {}",
        String::from_utf8_lossy(&t_out),
        String::from_utf8_lossy(&f_out)
    );
    assert_eq!(
        t_err,
        f_err,
        "{what}: stderr\n tetra {}\n facet {}",
        String::from_utf8_lossy(&t_err),
        String::from_utf8_lossy(&f_err)
    );
}
