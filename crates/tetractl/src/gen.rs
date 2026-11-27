//! `tetractl gen`: emit embedded plain-Rust templates keyed to `CONTRACT_SET`.
//!
//! These are committed artifacts, not derived from the overlay at runtime.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde_json::json;

use crate::error::CliError;
use crate::ncl::{self, INVALID_ARGUMENTS, OUTPUT_ERROR};
use crate::SCHEMA_VERSION;

const POD_PROFILE: &str = include_str!("../templates/pod_profile_gen.rs");
const DOCS_EOD: &str = include_str!("../templates/docs_eod.rs");

const POD_PROFILE_SHA: &str = "5c34d855b5012428350702e0b650de89f89879db3cda1fd9eb2abe5a01b828ad";
const DOCS_EOD_SHA: &str = "e013d039590950065a0006aaf8868e3eee97af154e59aad1ab8c67cda5b9b33d";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    PodProfile,
    DocsEod,
}

impl Target {
    fn parse(name: &str) -> Result<Self, CliError> {
        match name {
            "pod-profile" => Ok(Self::PodProfile),
            "docs-eod" => Ok(Self::DocsEod),
            other => Err(ncl::fail_msg(
                INVALID_ARGUMENTS,
                format!("unknown gen target: {other} (expected pod-profile or docs-eod)"),
            )),
        }
    }

    fn source(self) -> &'static str {
        match self {
            Self::PodProfile => POD_PROFILE,
            Self::DocsEod => DOCS_EOD,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::PodProfile => "pod-profile",
            Self::DocsEod => "docs-eod",
        }
    }

    fn sha256(self) -> &'static str {
        match self {
            Self::PodProfile => POD_PROFILE_SHA,
            Self::DocsEod => DOCS_EOD_SHA,
        }
    }
}

pub fn run(args: &[String], json_from_cli: bool) -> Result<(), CliError> {
    let json_hint = ncl::wants_json(args, json_from_cli);
    match parse(args, json_from_cli) {
        Ok(req) => match execute(&req) {
            Ok(()) => Ok(()),
            Err(e) => Err(ncl::emit_fail(&e, req.json)),
        },
        Err(e) => Err(ncl::emit_fail(&e, json_hint)),
    }
}

struct Request {
    target: Target,
    out: Option<String>,
    check: bool,
    json: bool,
}

fn parse(args: &[String], json_from_cli: bool) -> Result<Request, CliError> {
    let (json, quiet, rest) = ncl::strip_globals(args, json_from_cli)?;
    if quiet {
        return Err(ncl::fail_msg(
            INVALID_ARGUMENTS,
            "gen does not support --quiet",
        ));
    }
    let mut out: Option<String> = None;
    let mut check = false;
    let mut positionals: Vec<String> = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        let a = rest[i].as_str();
        if a == "--check" {
            check = true;
            i += 1;
            continue;
        }
        if let Some(v) = rest[i].strip_prefix("--out=") {
            if out.is_some() {
                return Err(ncl::fail_msg(
                    INVALID_ARGUMENTS,
                    "--out may only be specified once",
                ));
            }
            out = Some(v.to_owned());
            i += 1;
            continue;
        }
        if a == "--out" {
            i += 1;
            match rest.get(i) {
                None => return Err(ncl::fail_msg(INVALID_ARGUMENTS, "--out requires a value")),
                Some(v) => {
                    if out.is_some() {
                        return Err(ncl::fail_msg(
                            INVALID_ARGUMENTS,
                            "--out may only be specified once",
                        ));
                    }
                    out = Some(v.clone());
                    i += 1;
                    continue;
                }
            }
        }
        if a == "-" || !a.starts_with('-') {
            positionals.push(a.to_owned());
            i += 1;
            continue;
        }
        return Err(ncl::fail_msg(
            INVALID_ARGUMENTS,
            format!("unknown option: {a}"),
        ));
    }
    if positionals.len() != 1 {
        return Err(ncl::fail_msg(
            INVALID_ARGUMENTS,
            "gen requires a target: pod-profile or docs-eod",
        ));
    }
    if check && out.is_none() {
        return Err(ncl::fail_msg(
            INVALID_ARGUMENTS,
            "gen --check requires --out <path>",
        ));
    }
    Ok(Request {
        target: Target::parse(&positionals[0])?,
        out,
        check,
        json,
    })
}

fn execute(req: &Request) -> Result<(), CliError> {
    let body = req.target.source();
    if req.check {
        let path = req.out.as_deref().expect("checked");
        return check_out(path, body, req);
    }
    if let Some(path) = req.out.as_deref() {
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    ncl::fail_msg(OUTPUT_ERROR, format!("cannot write {path}: {e}"))
                })?;
            }
        }
        fs::write(path, body)
            .map_err(|e| ncl::fail_msg(OUTPUT_ERROR, format!("cannot write {path}: {e}")))?;
    } else if !req.json {
        io::stdout()
            .write_all(body.as_bytes())
            .map_err(|e| ncl::fail_msg(OUTPUT_ERROR, e.to_string()))?;
    }
    if req.json {
        ncl::emit_ok_json(&json!({
            "check": false,
            "contractSet": hedron_ncl::CONTRACT_SET,
            "out": req.out,
            "schemaVersion": SCHEMA_VERSION,
            "sha256": req.target.sha256(),
            "target": req.target.name(),
        }))?;
    }
    Ok(())
}

fn check_out(path: &str, expected: &str, req: &Request) -> Result<(), CliError> {
    let got = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(ncl::fail_msg(
                ncl::INVALID_WORKSPACE,
                format!("gen --check: {path} is missing"),
            ));
        }
        Err(e) => {
            return Err(ncl::fail_msg(
                OUTPUT_ERROR,
                format!("cannot read {path}: {e}"),
            ));
        }
    };
    if got != expected.as_bytes() {
        return Err(ncl::fail_msg(
            ncl::NCL_INVALID,
            format!(
                "gen --check: {path} drifts from embedded {} template",
                req.target.name()
            ),
        ));
    }
    if req.json {
        ncl::emit_ok_json(&json!({
            "check": true,
            "contractSet": hedron_ncl::CONTRACT_SET,
            "out": path,
            "schemaVersion": SCHEMA_VERSION,
            "sha256": req.target.sha256(),
            "target": req.target.name(),
        }))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ncl::sha256_hex;

    #[test]
    fn pod_profile_sha256_is_pinned() {
        assert_eq!(sha256_hex(POD_PROFILE.as_bytes()), POD_PROFILE_SHA);
        assert_eq!(POD_PROFILE, Target::PodProfile.source());
    }

    #[test]
    fn docs_eod_sha256_is_pinned() {
        assert_eq!(sha256_hex(DOCS_EOD.as_bytes()), DOCS_EOD_SHA);
        assert_eq!(DOCS_EOD, Target::DocsEod.source());
    }

    #[test]
    fn templates_are_keyed_to_contract_set() {
        let quoted = format!("\"{0}\"", hedron_ncl::CONTRACT_SET);
        assert!(
            POD_PROFILE.contains(&quoted),
            "pod-profile must embed hedron_ncl::CONTRACT_SET"
        );
        assert_eq!(sha256_hex(POD_PROFILE.as_bytes()), POD_PROFILE_SHA);
        assert_eq!(sha256_hex(DOCS_EOD.as_bytes()), DOCS_EOD_SHA);
        assert_eq!(
            sha256_hex(include_str!("../templates/docs_eod.rs").as_bytes()),
            DOCS_EOD_SHA
        );
    }

    #[test]
    fn emit_is_deterministic() {
        assert_eq!(Target::PodProfile.source(), Target::PodProfile.source());
        assert_eq!(Target::DocsEod.source(), Target::DocsEod.source());
    }

    #[test]
    fn emitted_text_has_no_vm() {
        for src in [POD_PROFILE, DOCS_EOD] {
            assert!(!src.contains("use nickel_lang_core"));
            assert!(!src.contains("hedron_ncl::"));
            assert!(!src.contains("extern crate nickel"));
        }
    }

    #[test]
    fn out_and_check_pass_and_drift() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pod.rs");
        let path_s = path.to_str().unwrap();
        execute(&Request {
            target: Target::PodProfile,
            out: Some(path_s.to_owned()),
            check: false,
            json: false,
        })
        .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), POD_PROFILE);
        execute(&Request {
            target: Target::PodProfile,
            out: Some(path_s.to_owned()),
            check: true,
            json: false,
        })
        .unwrap();
        fs::write(&path, "drift\n").unwrap();
        let err = execute(&Request {
            target: Target::PodProfile,
            out: Some(path_s.to_owned()),
            check: true,
            json: false,
        })
        .unwrap_err();
        assert_eq!(err.code(), ncl::NCL_INVALID);
        let missing = dir.path().join("gone.rs");
        let err = execute(&Request {
            target: Target::DocsEod,
            out: Some(missing.to_str().unwrap().to_owned()),
            check: true,
            json: false,
        })
        .unwrap_err();
        assert_eq!(err.code(), ncl::INVALID_WORKSPACE);
    }
}
