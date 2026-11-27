//! In-process `tetractl pack` via `hedron_ncl::pack::write_to`.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::error::CliError;
use crate::ncl::{self, OUTPUT_ERROR};
use crate::SCHEMA_VERSION;

pub fn run(args: &[String], json_from_cli: bool) -> Result<(), CliError> {
    let json_hint = ncl::wants_json(args, json_from_cli);
    match parse(args, json_from_cli) {
        Ok((out, json, quiet)) => match write_pack(&out, json, quiet) {
            Ok(()) => Ok(()),
            Err(e) => Err(ncl::emit_fail(&e, json)),
        },
        Err(e) => Err(ncl::emit_fail(&e, json_hint)),
    }
}

fn parse(args: &[String], json_from_cli: bool) -> Result<(String, bool, bool), CliError> {
    let (json, quiet, rest) = ncl::strip_globals(args, json_from_cli)?;
    let mut outs: Vec<String> = Vec::new();
    let mut positionals: Vec<String> = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        let a = rest[i].as_str();
        if let Some(v) = eq_value(a, "--out") {
            outs.push(v.unwrap_or_default());
            i += 1;
            continue;
        }
        if a == "--out" {
            i += 1;
            match rest.get(i) {
                None => {
                    return Err(ncl::fail_msg(
                        ncl::INVALID_ARGUMENTS,
                        "--out requires a value",
                    ))
                }
                Some(v) => {
                    outs.push(v.clone());
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
            ncl::INVALID_ARGUMENTS,
            format!("unknown option: {a}"),
        ));
    }
    if !positionals.is_empty() {
        return Err(ncl::fail_msg(
            ncl::INVALID_ARGUMENTS,
            "ncl pack takes no positional arguments; use --out <dir>",
        ));
    }
    if outs.is_empty() {
        return Err(ncl::fail_msg(
            ncl::INVALID_ARGUMENTS,
            "ncl pack requires --out <dir>",
        ));
    }
    if outs.len() > 1 {
        return Err(ncl::fail_msg(
            ncl::INVALID_ARGUMENTS,
            "--out may only be specified once",
        ));
    }
    Ok((outs.remove(0), json, quiet))
}

fn eq_value(arg: &str, flag: &str) -> Option<Option<String>> {
    arg.strip_prefix(&format!("{flag}="))
        .map(|rest| Some(rest.to_owned()))
}

fn write_pack(out: &str, json: bool, quiet: bool) -> Result<(), CliError> {
    let written = hedron_ncl::pack::write_to(Path::new(out))
        .map_err(|e| ncl::fail_msg(OUTPUT_ERROR, format!("cannot write {out}: {e}")))?;
    if quiet {
        return Ok(());
    }
    if json {
        let files: Vec<String> = hedron_ncl::pack::FILES
            .iter()
            .map(|f| format!("{}/{}", hedron_ncl::pack::ROOT, f.path))
            .collect();
        let envelope = json!({
            "contractSet": written.contract_set,
            "files": files,
            "importPath": out,
            "marker": PathBuf::from(out)
                .join(hedron_ncl::pack::ROOT)
                .join(hedron_ncl::CONTRACT_SET_MARKER_FILE)
                .display()
                .to_string(),
            "root": PathBuf::from(out)
                .join(hedron_ncl::pack::ROOT)
                .display()
                .to_string(),
            "schemaVersion": SCHEMA_VERSION,
        });
        ncl::emit_ok_json(&envelope)
    } else {
        let line1 = format!(
            "ok {} (contractSet={} files={})",
            written.root.display(),
            written.contract_set,
            hedron_ncl::pack::FILES.len()
        );
        let line2 = format!("import with: facet ncl <verb> <world.ncl> --import-path {out}");
        ncl::emit_ok_human(&format!("{line1}\n{line2}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_root(out: &str) -> PathBuf {
        PathBuf::from(out).join(hedron_ncl::pack::ROOT)
    }

    #[test]
    fn no_process_spawn() {
        let src = include_str!("pack.rs");
        let process = ["std", "process"].join("::");
        let spawn = ["Command", "new"].join("::");
        assert!(!src.contains(&process), "pack must not mention {process}");
        assert!(!src.contains(&spawn), "pack must not spawn {spawn}");
    }

    #[test]
    fn trailing_slash_out_uses_path_join() {
        assert_eq!(pack_root("dir/").display().to_string(), "dir/hedron-ncl");
        assert_eq!(pack_root("./x/").display().to_string(), "./x/hedron-ncl");
        assert_eq!(pack_root("x//").display().to_string(), "x//hedron-ncl");
        assert_ne!(
            format!("{}/{}", "dir/", hedron_ncl::pack::ROOT),
            pack_root("dir/").display().to_string()
        );
    }
}
