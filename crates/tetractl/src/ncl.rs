//! Shared CLI shape for in-process `check` and `eval` (Facet ncl parity).

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::CliError;
use crate::SCHEMA_VERSION;

pub const INVALID_ARGUMENTS: u8 = 2;
pub const INVALID_WORKSPACE: u8 = 3;
pub const NCL_INVALID: u8 = 5;
pub const OUTPUT_ERROR: u8 = 6;

const SECRET_SEGMENTS: &[&str] = &["token", "password", "api_key", "authorization", "secret"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verb {
    Check,
    Eval,
}

pub struct Request {
    pub path: PathBuf,
    pub json: bool,
    pub quiet: bool,
    pub overrides: Vec<String>,
    pub import_path: Option<PathBuf>,
}

#[derive(Serialize)]
struct FailEnvelope {
    error: FailBody,
    #[serde(rename = "schemaVersion")]
    schema_version: u64,
}

#[derive(Serialize)]
struct FailBody {
    category: &'static str,
    #[serde(rename = "exitCode")]
    exit_code: u8,
    message: String,
}

/// Parse forwarded check/eval args. `--json` / `-q` are stripped first (Facet).
pub fn parse(args: &[String], json_from_cli: bool, verb: Verb) -> Result<Request, CliError> {
    let (json, quiet, rest) = strip_globals(args, json_from_cli)?;
    let mut frozen = false;
    let mut overrides = Vec::new();
    let mut import_path: Option<String> = None;
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        let a = rest[i].as_str();
        if a == "--frozen" {
            frozen = true;
            i += 1;
            continue;
        }
        if let Some(v) = eq_value(a, "--var") {
            push_var(&mut overrides, v)?;
            i += 1;
            continue;
        }
        if a == "--var" {
            i += 1;
            match rest.get(i) {
                None => return Err(fail_msg(INVALID_ARGUMENTS, "--var requires a value")),
                Some(v) => {
                    push_var(&mut overrides, Some(v.clone()))?;
                    i += 1;
                    continue;
                }
            }
        }
        if let Some(v) = eq_value(a, "--import-path") {
            set_import(&mut import_path, v)?;
            i += 1;
            continue;
        }
        if a == "--import-path" {
            i += 1;
            match rest.get(i) {
                None => {
                    return Err(fail_msg(
                        INVALID_ARGUMENTS,
                        "--import-path requires a value",
                    ))
                }
                Some(v) => {
                    set_import(&mut import_path, Some(v.clone()))?;
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
        return Err(fail_msg(INVALID_ARGUMENTS, format!("unknown option: {a}")));
    }
    if frozen {
        return Err(freeze_error(verb));
    }
    if positionals.len() != 1 {
        return Err(fail_msg(
            INVALID_ARGUMENTS,
            "ncl requires <path> to a .ncl module",
        ));
    }
    Ok(Request {
        path: PathBuf::from(&positionals[0]),
        json,
        quiet,
        overrides,
        import_path: resolve_import(import_path)?,
    })
}

fn strip_globals(
    args: &[String],
    json_from_cli: bool,
) -> Result<(bool, bool, Vec<String>), CliError> {
    let mut json_count = u8::from(json_from_cli);
    let mut quiet_count: u8 = 0;
    let mut rest = Vec::new();
    for a in args {
        match a.as_str() {
            "--json" => json_count = json_count.saturating_add(1),
            "-q" | "--quiet" => quiet_count = quiet_count.saturating_add(1),
            _ => rest.push(a.clone()),
        }
    }
    if json_count > 1 {
        return Err(fail_msg(
            INVALID_ARGUMENTS,
            "--json may only be specified once",
        ));
    }
    if quiet_count > 1 {
        return Err(fail_msg(
            INVALID_ARGUMENTS,
            "--quiet may only be specified once",
        ));
    }
    let json = json_count == 1;
    let quiet = quiet_count == 1;
    if json && quiet {
        return Err(fail_msg(
            INVALID_ARGUMENTS,
            "--json cannot be combined with --quiet",
        ));
    }
    Ok((json, quiet, rest))
}

fn freeze_error(verb: Verb) -> CliError {
    match verb {
        Verb::Check => fail_msg(
            INVALID_ARGUMENTS,
            "ncl check does not support --frozen; use ncl export --frozen",
        ),
        Verb::Eval => fail_msg(
            INVALID_ARGUMENTS,
            "--frozen requires a prior ncl export recorded in Lattice for this module",
        ),
    }
}

fn eq_value(arg: &str, flag: &str) -> Option<Option<String>> {
    arg.strip_prefix(&format!("{flag}="))
        .map(|rest| Some(rest.to_owned()))
}

fn set_import(slot: &mut Option<String>, value: Option<String>) -> Result<(), CliError> {
    let v = value.ok_or_else(|| fail_msg(INVALID_ARGUMENTS, "--import-path requires a value"))?;
    if slot.is_some() {
        return Err(fail_msg(
            INVALID_ARGUMENTS,
            "--import-path may only be specified once",
        ));
    }
    *slot = Some(v);
    Ok(())
}

fn push_var(out: &mut Vec<String>, value: Option<String>) -> Result<(), CliError> {
    let raw = value.ok_or_else(|| fail_msg(INVALID_ARGUMENTS, "--var requires a value"))?;
    let Some((path, _)) = raw.split_once('=') else {
        return Err(fail_msg(
            INVALID_ARGUMENTS,
            format!("--var expects path.to.field=value, got \"{raw}\""),
        ));
    };
    for seg in path.split('.') {
        if SECRET_SEGMENTS.iter().any(|s| seg.eq_ignore_ascii_case(s)) {
            return Err(fail_msg(
                INVALID_ARGUMENTS,
                format!(
                    "refuses secret field override `{path}`; use secret_ref and Facet env hydration instead"
                ),
            ));
        }
    }
    out.push(raw);
    Ok(())
}

fn resolve_import(cli: Option<String>) -> Result<Option<PathBuf>, CliError> {
    let chosen = cli
        .filter(|s| !s.is_empty())
        .or_else(|| nonempty_env("TETRA_NCL_IMPORT_PATH"))
        .or_else(|| nonempty_env("FACET_NCL_IMPORT_PATH"));
    let Some(raw) = chosen else {
        return Ok(None);
    };
    let path = PathBuf::from(&raw);
    if !path.is_dir() {
        return Err(fail_msg(
            INVALID_ARGUMENTS,
            format!("--import-path {raw} is not a directory"),
        ));
    }
    let drift =
        hedron_ncl::pack::verify(&path).map_err(|e| fail_msg(INVALID_ARGUMENTS, e.to_string()))?;
    if !drift.is_empty() {
        return Err(fail_msg(
            INVALID_ARGUMENTS,
            format!("import path {raw} pack drift: {drift:?}"),
        ));
    }
    Ok(Some(path))
}

fn nonempty_env(key: &str) -> Option<String> {
    env::var(key).ok().filter(|s| !s.is_empty())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

pub fn canonical_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

pub fn read_source(path: &Path) -> Result<Vec<u8>, CliError> {
    fs::read(path).map_err(|e| {
        fail_msg(
            INVALID_WORKSPACE,
            format!("cannot read {}: {e}", path.display()),
        )
    })
}

pub fn map_eval_error(err: hedron_ncl::Error) -> CliError {
    match err {
        hedron_ncl::Error::Override(msg) => fail_msg(INVALID_ARGUMENTS, format!("--var: {msg}")),
        hedron_ncl::Error::Build(msg) => fail_msg(INVALID_WORKSPACE, msg),
        hedron_ncl::Error::Nickel(msg) => fail_msg(NCL_INVALID, msg.trim_end().to_owned()),
        hedron_ncl::Error::Export(msg) => fail_msg(NCL_INVALID, msg),
        hedron_ncl::Error::Json(e) => fail_msg(NCL_INVALID, e.to_string()),
    }
}

pub fn fail_msg(code: u8, message: impl Into<String>) -> CliError {
    CliError::engine(message.into(), code)
}

pub fn emit_fail(err: &CliError, json: bool) -> CliError {
    let code = err.code();
    let message = err.message().to_owned();
    let category = match code {
        INVALID_ARGUMENTS => "invalid_arguments",
        INVALID_WORKSPACE => "invalid_workspace",
        NCL_INVALID => "ncl_invalid",
        OUTPUT_ERROR => "output_error",
        _ => "invalid_arguments",
    };
    let _ = write_fail(json, category, code, &message);
    CliError::engine(String::new(), code)
}

fn write_fail(json: bool, category: &'static str, code: u8, message: &str) -> io::Result<()> {
    if json {
        let env = FailEnvelope {
            error: FailBody {
                category,
                exit_code: code,
                message: message.to_owned(),
            },
            schema_version: SCHEMA_VERSION,
        };
        let body = serde_json::to_string_pretty(&env).unwrap_or_else(|_| "{}".to_owned());
        writeln!(io::stdout().lock(), "{body}")
    } else {
        writeln!(io::stderr().lock(), "error[{category}]: {message}")
    }
}

pub fn emit_ok_json(value: &Value) -> Result<(), CliError> {
    let body =
        serde_json::to_string_pretty(value).map_err(|e| fail_msg(OUTPUT_ERROR, e.to_string()))?;
    writeln!(io::stdout().lock(), "{body}").map_err(|e| fail_msg(OUTPUT_ERROR, e.to_string()))
}

pub fn emit_ok_human(line: &str) -> Result<(), CliError> {
    writeln!(io::stdout().lock(), "{line}").map_err(|e| fail_msg(OUTPUT_ERROR, e.to_string()))
}

/// Whether this arg list asked for JSON (global clap plus forwarded `--json`).
pub fn wants_json(args: &[String], json_from_cli: bool) -> bool {
    json_from_cli || args.iter().any(|a| a == "--json")
}
