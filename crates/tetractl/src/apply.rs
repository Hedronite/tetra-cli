//! `tetractl apply`: evaluate once, freeze a schemaVersion 1 bundle, spawn Facet.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;

use serde_json::{json, Value};

use crate::error::CliError;
use crate::facet::{self, FacetOutput};
use crate::ncl::{self, Request, Verb};
use crate::SCHEMA_VERSION;

pub fn run(args: &[String], json_from_cli: bool) -> Result<(), CliError> {
    let json_hint = ncl::wants_json(args, json_from_cli);
    let req = match ncl::parse(args, json_from_cli, Verb::Apply) {
        Ok(parsed) => parsed,
        Err(error) => return Err(ncl::emit_fail(&error, json_hint)),
    };
    let bundle = match freeze(&req) {
        Ok(bytes) => bytes,
        Err(error) => return Err(ncl::emit_fail(&error, req.json)),
    };
    let output = spawn_frozen(&req, &bundle)?;
    write_passthrough(&output)
}

fn freeze(req: &Request) -> Result<Vec<u8>, CliError> {
    let source = ncl::read_source(&req.path)?;
    let module_hash = ncl::sha256_hex(&source);
    let imports = req
        .import_path
        .as_ref()
        .map(|path| vec![path.clone()])
        .unwrap_or_default();
    let full = hedron_ncl::eval::export(
        hedron_ncl::eval::Input::Path(&req.path),
        &req.overrides,
        &imports,
    )
    .map_err(ncl::map_eval_error)?;
    let Some(record) = full.as_object() else {
        return Err(ncl::fail_msg(
            ncl::NCL_INVALID,
            format!(
                "{}: export must be a record (World = {{ cluster, intent, calls }}), got {}",
                req.path.display(),
                json_kind(&full)
            ),
        ));
    };
    let export_hash = ncl::sha256_hex(ncl::canonical_json(&full).as_bytes());
    let snapshot = source_snapshot(&req.path)?;
    let var_names: Vec<String> = req
        .overrides
        .iter()
        .map(|item| {
            item.split_once('=')
                .map(|(path, _)| path.to_owned())
                .unwrap_or_else(|| item.clone())
        })
        .collect();
    let bundle = json!({
        "schemaVersion": SCHEMA_VERSION,
        "path": req.path.display().to_string(),
        "moduleHash": module_hash,
        "exportHash": export_hash,
        "contractSet": hedron_ncl::CONTRACT_SET,
        "varNames": var_names,
        "sourceSnapshot": snapshot,
        "cluster": record.get("cluster").cloned().unwrap_or(Value::Null),
        "intent": record.get("intent").cloned().unwrap_or(Value::Null),
        "calls": record.get("calls").cloned().unwrap_or(Value::Null),
    });
    serde_json::to_vec(&bundle).map_err(|error| ncl::fail_msg(ncl::OUTPUT_ERROR, error.to_string()))
}

fn source_snapshot(path: &Path) -> Result<Value, CliError> {
    let absolute = std::path::absolute(path).map_err(|error| {
        ncl::fail_msg(
            ncl::INVALID_WORKSPACE,
            format!("cannot resolve {}: {error}", path.display()),
        )
    })?;
    let dir = absolute.parent().unwrap_or_else(|| Path::new("."));
    let mut sources = BTreeMap::new();
    let entries = std::fs::read_dir(dir).map_err(|error| {
        ncl::fail_msg(
            ncl::INVALID_WORKSPACE,
            format!("cannot read {}: {error}", dir.display()),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            ncl::fail_msg(
                ncl::INVALID_WORKSPACE,
                format!("cannot read {}: {error}", dir.display()),
            )
        })?;
        let file_path = entry.path();
        if file_path.extension().is_some_and(|ext| ext == "ncl") {
            let bytes = std::fs::read(&file_path).map_err(|error| {
                ncl::fail_msg(
                    ncl::INVALID_WORKSPACE,
                    format!("cannot read {}: {error}", file_path.display()),
                )
            })?;
            sources.insert(
                file_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                String::from_utf8_lossy(&bytes).into_owned(),
            );
        }
    }
    Ok(json!({
        "entry": absolute
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        "sources": sources,
    }))
}

fn spawn_frozen(req: &Request, bundle: &[u8]) -> Result<FacetOutput, CliError> {
    let mut args = vec!["apply".to_owned(), "--frozen".to_owned(), "-".to_owned()];
    if req.json {
        args.push("--json".to_owned());
    }
    if req.quiet {
        args.push("-q".to_owned());
    }
    facet::exec_with_stdin(args, bundle, req.json, true)
}

fn write_passthrough(output: &FacetOutput) -> Result<(), CliError> {
    let _ = io::stderr().write_all(&output.stderr);
    let _ = io::stdout().write_all(&output.stdout);
    if output.exit_code == 0 {
        Ok(())
    } else {
        Err(CliError::engine(String::new(), output.exit_code))
    }
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "a record",
    }
}
