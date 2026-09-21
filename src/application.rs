use std::path::PathBuf;

use anyhow::Result;
use tracing::warn;

/// A request from any MyConnect frontend.
#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    /// Start the long-running MyConnect service.
    Run(RunRequest),
    /// Send a file to a device.
    Send(SendRequest),
}

/// Options for starting the MyConnect service.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RunRequest {
    /// Directory in which received files should be stored.
    pub download_dir: Option<PathBuf>,
}

/// Options for sending a file to a device.
#[derive(Debug, PartialEq, Eq)]
pub struct SendRequest {
    /// Device name or identifier selected by the user.
    pub device: String,
    /// File to send.
    pub file: PathBuf,
}

/// Execute a frontend request.
///
/// This is the seam shared by the CLI and future GUI binaries. Protocol,
/// discovery, and transfer implementations will be connected here as they are
/// introduced.
pub async fn execute(request: Request) -> Result<()> {
    match request {
        Request::Run(request) => {
            warn!(?request.download_dir, "run service is not implemented yet");
        }
        Request::Send(request) => {
            warn!(device = %request.device, file = %request.file.display(), "file transfer is not implemented yet");
        }
    }

    Ok(())
}
