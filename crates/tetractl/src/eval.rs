//! In-process `tetractl check` and `tetractl eval` via `hedron-ncl`.

use std::path::Path;

use serde_json::{json, Value};

use crate::error::CliError;
use crate::ncl::{self, Request, Verb};
use crate::SCHEMA_VERSION;

pub fn run(verb: Verb, args: &[String], json_from_cli: bool) -> Result<(), CliError> {
    let json_hint = ncl::wants_json(args, json_from_cli);
    let req = match ncl::parse(args, json_from_cli, verb) {
        Ok(r) => r,
        Err(e) => return Err(ncl::emit_fail(&e, json_hint)),
    };
    match execute(verb, &req) {
        Ok(()) => Ok(()),
        Err(e) => Err(ncl::emit_fail(&e, req.json)),
    }
}

/// Facet-shaped export envelope for `status` / `diff` (always JSON semantics).
pub fn export_json(path: &Path, rest: &[String]) -> Result<Value, CliError> {
    let mut args: Vec<String> = vec![path.display().to_string()];
    args.extend(rest.iter().cloned());
    let req = match ncl::parse(&args, true, Verb::Eval) {
        Ok(r) => r,
        Err(e) => return Err(ncl::emit_fail(&e, true)),
    };
    match envelope(Verb::Eval, &req) {
        Ok(v) => Ok(v),
        Err(e) => Err(ncl::emit_fail(&e, true)),
    }
}

fn execute(verb: Verb, req: &Request) -> Result<(), CliError> {
    let value = envelope(verb, req)?;
    if req.quiet {
        return Ok(());
    }
    if req.json {
        ncl::emit_ok_json(&value)
    } else {
        ncl::emit_ok_human(&human_line(verb, req, &value))
    }
}

fn envelope(verb: Verb, req: &Request) -> Result<Value, CliError> {
    let source = ncl::read_source(&req.path)?;
    let module_hash = ncl::sha256_hex(&source);
    let imports = req
        .import_path
        .as_ref()
        .map(|p| vec![p.clone()])
        .unwrap_or_default();
    let input = hedron_ncl::eval::Input::Path(&req.path);
    match verb {
        Verb::Check => {
            hedron_ncl::eval::check(input, &req.overrides, &imports)
                .map_err(ncl::map_eval_error)?;
            Ok(json!({
                "contractSet": hedron_ncl::CONTRACT_SET,
                "moduleHash": module_hash,
                "path": req.path.display().to_string(),
                "schemaVersion": SCHEMA_VERSION,
            }))
        }
        Verb::Apply => unreachable!("apply does not use eval envelope"),
        Verb::Eval => {
            let full = hedron_ncl::eval::export(input, &req.overrides, &imports)
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
            Ok(json!({
                "calls": record.get("calls").cloned().unwrap_or(Value::Null),
                "cluster": record.get("cluster").cloned().unwrap_or(Value::Null),
                "contractSet": hedron_ncl::CONTRACT_SET,
                "exportHash": export_hash,
                "intent": record.get("intent").cloned().unwrap_or(Value::Null),
                "moduleHash": module_hash,
                "path": req.path.display().to_string(),
                "schemaVersion": SCHEMA_VERSION,
            }))
        }
    }
}

fn json_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "a record",
    }
}

fn human_line(verb: Verb, req: &Request, value: &Value) -> String {
    let path = req.path.display();
    let module_hash = value["moduleHash"].as_str().unwrap_or("");
    let contract = value["contractSet"].as_str().unwrap_or("");
    match verb {
        Verb::Check => format!("ok {path} (moduleHash={module_hash} contractSet={contract})"),
        Verb::Eval => {
            let export_hash = value["exportHash"].as_str().unwrap_or("");
            format!(
                "ok {path} (moduleHash={module_hash} exportHash={export_hash} contractSet={contract})"
            )
        }
        Verb::Apply => unreachable!("apply does not use eval human_line"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn no_process_spawn() {
        let src = concat!(include_str!("ncl.rs"), include_str!("eval.rs"));
        let process = ["std", "process"].join("::");
        let spawn = ["Command", "new"].join("::");
        assert!(
            !src.contains(&process),
            "in-process module must not mention {process}"
        );
        assert!(
            !src.contains(&spawn),
            "in-process module must not spawn {spawn}"
        );
    }
}
