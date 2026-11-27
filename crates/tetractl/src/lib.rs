mod apply;
mod blame;
mod cli;
mod diff;
mod doctor;
mod error {
    use std::process::ExitCode;

    #[derive(Debug)]
    pub struct CliError {
        message: String,
        code: u8,
    }

    impl CliError {
        pub fn usage(message: impl Into<String>) -> Self {
            Self {
                message: message.into(),
                code: 2,
            }
        }

        pub fn engine(message: impl Into<String>, code: u8) -> Self {
            Self {
                message: message.into(),
                code,
            }
        }

        pub fn code(&self) -> u8 {
            self.code
        }

        pub fn message(&self) -> &str {
            &self.message
        }

        pub fn exit(self) -> ExitCode {
            if !self.message.is_empty() {
                eprintln!("{}", self.message);
            }
            ExitCode::from(self.code)
        }
    }
}
mod eval;
mod facet;
mod gen;
mod ncl;
mod pack;
mod profile;
mod session;
mod status;

pub use error::CliError;

pub const SCHEMA_VERSION: u64 = 1;

/// Binary name (`tetractl` or `tetra`) from argv0.
pub fn bin_name() -> &'static str {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        let argv0 = std::env::args()
            .next()
            .unwrap_or_else(|| "tetractl".to_owned());
        std::path::Path::new(&argv0)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("tetractl")
            .to_owned()
    })
    .as_str()
}

pub fn run(invoked_as: &str) -> Result<(), CliError> {
    cli::execute(invoked_as)
}
