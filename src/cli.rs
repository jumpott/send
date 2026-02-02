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
        /// Patterns to exclude (e.g. "*.git", "node_modules")
        #[arg(short, long)]
        exclude: Vec<String>,
    },
    /// List transfer history
    List,
    /// Resume a transfer
    Resume {
        /// ID of the transfer to resume
        id: i64,
        /// Update exclude patterns
        #[arg(short, long)]
        exclude: Vec<String>,
    },
    /// Restart a transfer (re-scan and re-send)
    Restart {
        /// ID of the transfer to restart
        id: i64,
        /// Update exclude patterns
        #[arg(short, long)]
        exclude: Vec<String>,
    },
    /// Remove a transfer history
    Remove {
        /// ID of the transfer to remove
        id: i64,
    },
}
