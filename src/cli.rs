use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Receive files/folders
    Serve {
        /// Directory to save received files
        path: PathBuf,
        /// Port to listen on
        port: u16,
    },
    /// Send files/folders
    Push {
        /// File or directory to send
        path: PathBuf,
        /// Target IP
        ip: String,
        /// Target Port
        port: u16,
    },
    /// List transfer history
    List,
    /// Resume a transfer
    Resume {
        /// ID of the transfer to resume
        id: i64,
    },
    /// Restart a transfer (re-scan and re-send)
    Restart {
        /// ID of the transfer to restart
        id: i64,
    },
    /// Remove a transfer history
    Remove {
        /// ID of the transfer to remove
        id: i64,
    },
}
