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

    pub fn exit(self) -> ExitCode {
        if !self.message.is_empty() {
            eprintln!("{}", self.message);
        }
        ExitCode::from(self.code)
    }
}
