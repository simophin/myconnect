use std::path::PathBuf;

use clap::{Parser, Subcommand};
use myconnect::application::{Request, RunRequest, SendRequest};

/// Connect and communicate with your devices.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the MyConnect service.
    Run {
        /// Directory in which received files will be stored.
        #[arg(long, value_name = "DIRECTORY")]
        download_dir: Option<PathBuf>,
    },
    /// Send a file to a device.
    Send {
        /// Device name or identifier.
        device: String,
        /// File to send.
        file: PathBuf,
    },
}

impl From<Cli> for Request {
    fn from(cli: Cli) -> Self {
        match cli.command {
            Command::Run { download_dir } => Request::Run(RunRequest { download_dir }),
            Command::Send { device, file } => Request::Send(SendRequest { device, file }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn parses_run_command() {
        let cli = Cli::try_parse_from(["myconnect", "run", "--download-dir", "/tmp"])
            .expect("run command should parse");

        assert_eq!(
            Request::from(cli),
            Request::Run(RunRequest {
                download_dir: Some(PathBuf::from("/tmp")),
            })
        );
    }

    #[test]
    fn parses_send_command() {
        let cli = Cli::try_parse_from(["myconnect", "send", "phone", "photo.jpg"])
            .expect("send command should parse");

        assert_eq!(
            Request::from(cli),
            Request::Send(SendRequest {
                device: "phone".to_owned(),
                file: PathBuf::from("photo.jpg"),
            })
        );
    }
}
