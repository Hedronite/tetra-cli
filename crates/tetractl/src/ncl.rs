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

#[derive(Debug)]
pub struct Request {
    pub path: PathBuf,
    pub json: bool,
    pub quiet: bool,
    pub overrides: Vec<String>,
    pub import_path: Option<PathBuf>,
}

struct Tokens {
    json: bool,
    quiet: bool,
    frozen: u8,
    vars: Vec<String>,
    imports: Vec<String>,
    positionals: Vec<String>,
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
    let tokens = collect_tokens(args, json_from_cli)?;
    if tokens.positionals.len() != 1 {
        return Err(fail_msg(
            INVALID_ARGUMENTS,
            "ncl requires <path> to a .ncl module",
        ));
    }
    let overrides = check_vars(&tokens.vars)?;
    let import_path = resolve_imports(&tokens.imports)?;
    if tokens.frozen > 0 {
        return Err(freeze_error(verb));
    }
    Ok(Request {
        path: PathBuf::from(&tokens.positionals[0]),
        json: tokens.json,
        quiet: tokens.quiet,
        overrides,
        import_path,
    })
}

fn collect_tokens(args: &[String], json_from_cli: bool) -> Result<Tokens, CliError> {
    let (json, quiet, rest) = strip_globals(args, json_from_cli)?;
    let mut frozen: u8 = 0;
    let mut vars = Vec::new();
    let mut imports = Vec::new();
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        let a = rest[i].as_str();
        if a == "--frozen" {
            frozen = frozen.saturating_add(1);
            if frozen > 1 {
                return Err(fail_msg(
                    INVALID_ARGUMENTS,
                    "--frozen may only be specified once",
                ));
            }
            i += 1;
            continue;
        }
        if let Some(v) = eq_value(a, "--var") {
            vars.push(v.unwrap_or_default());
            i += 1;
            continue;
        }
        if a == "--var" {
            i += 1;
            match rest.get(i) {
                None => return Err(fail_msg(INVALID_ARGUMENTS, "--var requires a value")),
                Some(v) => {
                    vars.push(v.clone());
                    i += 1;
                    continue;
                }
            }
        }
        if let Some(v) = eq_value(a, "--import-path") {
            imports.push(v.unwrap_or_default());
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
                    imports.push(v.clone());
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
    Ok(Tokens {
        json,
        quiet,
        frozen,
        vars,
        imports,
        positionals,
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
            "--json and --quiet cannot be used together",
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

fn check_vars(raw: &[String]) -> Result<Vec<String>, CliError> {
    for item in raw {
        if !item.contains('=') {
            return Err(fail_msg(
                INVALID_ARGUMENTS,
                format!("--var expects path.to.field=value, got \"{item}\""),
            ));
        }
    }
    for item in raw {
        let path = item.split_once('=').map(|(p, _)| p).unwrap_or(item);
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
    }
    Ok(raw.to_vec())
}

fn resolve_imports(cli: &[String]) -> Result<Option<PathBuf>, CliError> {
    if cli.len() > 1 {
        return Err(fail_msg(
            INVALID_ARGUMENTS,
            "--import-path may only be specified once",
        ));
    }
    match cli.first() {
        None => resolve_import(None),
        Some(raw) => resolve_import(Some(raw.as_str())),
    }
}

/// `None` = flag absent (env fallback). `Some("")` = CLI empty, suppress env.
fn resolve_import(cli: Option<&str>) -> Result<Option<PathBuf>, CliError> {
    let chosen = match cli {
        Some("") => return Ok(None),
        Some(raw) => raw.to_owned(),
        None => match nonempty_env("TETRA_NCL_IMPORT_PATH")
            .or_else(|| nonempty_env("FACET_NCL_IMPORT_PATH"))
        {
            Some(v) => v,
            None => return Ok(None),
        },
    };
    let path = PathBuf::from(&chosen);
    if !path.is_dir() {
        return Err(fail_msg(
            INVALID_ARGUMENTS,
            format!("--import-path {chosen} is not a directory"),
        ));
    }
    let drift = hedron_ncl::pack::verify(&path).map_err(|e| {
        fail_msg(
            INVALID_ARGUMENTS,
            format!("import path {chosen} is not a materialized pack: {e}"),
        )
    })?;
    if !drift.is_empty() {
        return Err(fail_msg(
            INVALID_ARGUMENTS,
            format!("import path {chosen} pack drift: {drift:?}"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_owned()).collect()
    }

    fn err_msg(args: &[&str], json: bool, verb: Verb) -> (u8, String) {
        let e = parse(&s(args), json, verb).expect_err("expected parse error");
        (e.code(), e.message().to_owned())
    }

    #[test]
    fn b1_json_and_quiet_exact_text() {
        let (code, msg) = err_msg(&["world.ncl", "--json", "-q"], false, Verb::Check);
        assert_eq!(code, INVALID_ARGUMENTS);
        assert_eq!(msg, "--json and --quiet cannot be used together");
        let (code, msg) = err_msg(&["world.ncl", "-q", "--json"], false, Verb::Check);
        assert_eq!(code, INVALID_ARGUMENTS);
        assert_eq!(msg, "--json and --quiet cannot be used together");
        let (code, msg) = err_msg(
            &["world.ncl", "--import-path", "pack", "--json", "-q"],
            false,
            Verb::Eval,
        );
        assert_eq!(code, INVALID_ARGUMENTS);
        assert_eq!(msg, "--json and --quiet cannot be used together");
    }

    #[test]
    fn b2_empty_cli_import_suppresses_env() {
        let _g = ENV_LOCK.lock().unwrap();
        env::set_var("FACET_NCL_IMPORT_PATH", "pack");
        env::remove_var("TETRA_NCL_IMPORT_PATH");
        let req = parse(
            &s(&["tetra/world.ncl", "--import-path", "", "--json"]),
            false,
            Verb::Check,
        )
        .expect("empty CLI import is not an error");
        assert!(
            req.import_path.is_none(),
            "empty CLI must not fall back to env"
        );
        let req = parse(
            &s(&["tetra/world.ncl", "--import-path=", "--json"]),
            false,
            Verb::Check,
        )
        .expect("--import-path= is empty CLI");
        assert!(req.import_path.is_none());
        env::remove_var("FACET_NCL_IMPORT_PATH");
    }

    #[test]
    fn b3_precedence_loop_then_positional_then_var_then_frozen() {
        let (code, msg) = err_msg(&["--var", "noeq", "--nope", "--json"], false, Verb::Check);
        assert_eq!(code, INVALID_ARGUMENTS);
        assert_eq!(msg, "unknown option: --nope");

        let (code, msg) = err_msg(&["--frozen", "--json"], false, Verb::Check);
        assert_eq!(code, INVALID_ARGUMENTS);
        assert_eq!(msg, "ncl requires <path> to a .ncl module");

        let (code, msg) = err_msg(&["--frozen", "--json"], false, Verb::Eval);
        assert_eq!(code, INVALID_ARGUMENTS);
        assert_eq!(msg, "ncl requires <path> to a .ncl module");

        let (code, msg) = err_msg(&["--var", "noeq", "--json"], false, Verb::Check);
        assert_eq!(code, INVALID_ARGUMENTS);
        assert_eq!(msg, "ncl requires <path> to a .ncl module");

        let (code, msg) = err_msg(
            &["--import-path", "pack", "--import-path", "pack", "--json"],
            false,
            Verb::Check,
        );
        assert_eq!(code, INVALID_ARGUMENTS);
        assert_eq!(msg, "ncl requires <path> to a .ncl module");

        let (code, msg) = err_msg(
            &[
                "world.ncl",
                "--import-path",
                "pack",
                "--var",
                "calls.token=1",
                "--var",
                "noeq",
                "--json",
            ],
            false,
            Verb::Check,
        );
        assert_eq!(code, INVALID_ARGUMENTS);
        assert_eq!(msg, "--var expects path.to.field=value, got \"noeq\"");

        let (code, msg) = err_msg(
            &["world.ncl", "--frozen", "--frozen", "--json"],
            false,
            Verb::Eval,
        );
        assert_eq!(code, INVALID_ARGUMENTS);
        assert_eq!(msg, "--frozen may only be specified once");
    }

    #[test]
    fn b4_verify_err_uses_materialized_pack_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let pack = dir.path().join("pack_unreadable");
        fs::create_dir(&pack).unwrap();
        let mut perms = fs::metadata(&pack).unwrap().permissions();
        let world = dir.path().join("world.ncl");
        fs::write(&world, "{}\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o000);
            fs::set_permissions(&pack, perms).unwrap();
        }
        let args = s(&[
            world.to_str().unwrap(),
            "--import-path",
            pack.to_str().unwrap(),
            "--json",
        ]);
        let err = parse(&args, false, Verb::Check).expect_err("unreadable pack");
        assert_eq!(err.code(), INVALID_ARGUMENTS);
        let msg = err.message();
        assert!(
            msg.starts_with(&format!(
                "import path {} is not a materialized pack: ",
                pack.display()
            )),
            "got {msg}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = fs::metadata(&pack).unwrap().permissions();
            p.set_mode(0o755);
            fs::set_permissions(&pack, p).unwrap();
        }
    }
}
