use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};

use crate::error::CliError;
use crate::{facet, session};

pub const WRAP_ABOUT: &str =
    "World command for the Hedronite platform. Tetra hosts the only Nickel VM.";

pub const WRAP_LONG_ABOUT: &str = "\
Nickel is the language. tetractl (and the tetra binary alias) is the world command.

Tetra hosts the platform's only Nickel VM through hedron-ncl. check, eval, pack, and gen \
run in-process. apply still hands off to Facet. Tetra does not open Lattice.

Cargo.lock carries exactly one nickel-lang-core, exactly one hedron-ncl, and zero rusqlite. \
gen emits embedded plain-Rust templates keyed to CONTRACT_SET, not overlay-derived code.";

#[derive(Parser, Debug)]
#[command(
    name = "tetractl",
    version,
    about = WRAP_ABOUT,
    long_about = WRAP_LONG_ABOUT,
    disable_help_subcommand = true,
)]
pub struct Cli {
    /// Emit Facet JSON (schemaVersion 1) on stdout.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Parse and typecheck a Nickel world module in-process.
    Check {
        /// Path, then `--var` / `--import-path` / `--json` / `-q`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Export a Nickel world module in-process via hedron-ncl.
    Eval {
        /// Path, then `--var` / `--import-path` / `--json` / `-q`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Write the embedded hedron-ncl pack under `--out`.
    Pack {
        /// `--out <dir>`, `--json`, `-q`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Emit an embedded plain-Rust template keyed to CONTRACT_SET.
    Gen {
        /// `pod-profile` or `docs-eod`, then `--out` / `--check` / `--json`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Apply a Nickel world module (hands off to `facet ncl apply`).
    Apply {
        /// Path to a .ncl module.
        path: PathBuf,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
    /// Capability and host-law preflight (never prints secrets).
    Doctor {
        /// Probe the pack without writing .tetra/pack/ in the current directory.
        #[arg(long)]
        no_materialize: bool,
    },
    /// Read cluster, intent, and calls by names from a World export.
    Status {
        /// Path to a .ncl module.
        path: PathBuf,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
    /// Compare desired export against observed status; exit 1 on drift.
    Diff {
        /// Path to a .ncl module.
        path: PathBuf,
        /// Human YAML view of desired export only (never POST/apply).
        #[arg(long)]
        yaml: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
    /// Parse an admission error (h3s Status JSON or kubectl stderr; never evaluates Nickel).
    Blame {
        /// Status JSON, kubectl stderr, or `-` for stdin.
        input: Option<String>,
    },
    /// Lattice session join key (wraps `facet session`).
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
}

#[derive(Subcommand, Debug)]
enum SessionCommand {
    /// Start a session; prints ULID and sets TETRA_SESSION = FACET_SESSION.
    Start {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
    /// End a session (default: current TETRA_SESSION).
    End {
        /// Session ULID or `current`.
        id: Option<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
}

pub fn execute(invoked_as: &str) -> Result<(), CliError> {
    let cli = Cli::parse_from(prepend_name(invoked_as));
    match cli.command {
        None => {
            let mut cmd = Cli::command();
            cmd.print_help()
                .map_err(|error| CliError::usage(error.to_string()))?;
            println!();
            Ok(())
        }
        Some(Command::Check { args }) => crate::eval::run(crate::ncl::Verb::Check, &args, cli.json),
        Some(Command::Eval { args }) => crate::eval::run(crate::ncl::Verb::Eval, &args, cli.json),
        Some(Command::Pack { args }) => crate::pack::run(&args, cli.json),
        Some(Command::Gen { args }) => crate::gen::run(&args, cli.json),
        Some(Command::Apply { path, rest }) => facet::run("apply", &path, &rest, cli.json),
        Some(Command::Doctor { no_materialize }) => crate::doctor::run(cli.json, !no_materialize),
        Some(Command::Status { path, rest }) => {
            let (rest, json, _) = split_view_flags(rest, cli.json, false);
            crate::status::run(&path, &rest, json)
        }
        Some(Command::Diff { path, yaml, rest }) => {
            let (rest, json, yaml) = split_view_flags(rest, cli.json, yaml);
            crate::diff::run(&path, &rest, json, yaml)
        }
        Some(Command::Blame { input }) => crate::blame::run(input.as_deref(), cli.json),
        Some(Command::Session { command }) => match command {
            SessionCommand::Start { rest } => session::start(&rest, cli.json),
            SessionCommand::End { id, rest } => session::end(id.as_deref(), &rest, cli.json),
        },
    }
}

/// `--json` / `--yaml` after the path land in the trailing forwarded args;
/// for tetra-owned verbs they are tetra's view flags, not facet's.
fn split_view_flags(rest: Vec<String>, json: bool, yaml: bool) -> (Vec<String>, bool, bool) {
    let mut json = json;
    let mut yaml = yaml;
    let rest = rest
        .into_iter()
        .filter(|arg| match arg.as_str() {
            "--json" => {
                json = true;
                false
            }
            "--yaml" => {
                yaml = true;
                false
            }
            _ => true,
        })
        .collect();
    (rest, json, yaml)
}

fn prepend_name(invoked_as: &str) -> Vec<String> {
    let mut argv: Vec<String> = std::env::args().collect();
    if argv.is_empty() {
        argv.push(invoked_as.to_owned());
    } else {
        argv[0] = invoked_as.to_owned();
    }
    argv
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn help_mentions_in_process_vm() {
        let help = Cli::command().render_long_help().to_string();
        assert!(
            help.contains("hedron-ncl"),
            "help should name hedron-ncl as the VM host"
        );
        assert!(
            help.contains("in-process"),
            "help should say check/eval/pack/gen run in-process"
        );
        assert!(
            help.contains("nickel-lang-core"),
            "help should mention the exact-1 nickel-lang-core lock law"
        );
    }

    #[test]
    fn global_json_flag_is_documented() {
        let help = Cli::command().render_help().to_string();
        assert!(help.contains("--json"), "help should document --json");
    }
}
