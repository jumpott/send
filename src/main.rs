mod cli;
mod client;
mod db;
mod protocol;
mod server;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use db::Db;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = Db::init()?;

    match cli.command {
        Commands::Serve { path, port } => {
            server::run_server(path, port).await?;
        }
        Commands::Push {
            path,
            ip,
            port,
            exclude,
        } => {
            let abs_path = std::fs::canonicalize(&path).unwrap_or(path.clone());

            let exclude_json = if !exclude.is_empty() {
                Some(serde_json::to_string(&exclude)?)
            } else {
                None
            };

            let id = db.add_transfer(&abs_path.to_string_lossy(), &ip, port, exclude_json)?;
            println!("Transfer started with ID: {}", id);

            let log = db::TransferLog::new(id)?;
            client::scan_files(abs_path.clone(), &log, &exclude).await?;
            db.set_listing_complete(id, true)?;

            match client::send_pending_files(abs_path, ip, port, &log, &exclude).await {
                Ok(_) => {
                    db.update_status(id, "Completed")?;
                    println!("Transfer completed successfully.");
                }
                Err(e) => {
                    db.update_status(id, "Failed")?;
                    eprintln!("\nTransfer failed: {}", e);
                }
            }
        }
        Commands::List => {
            let transfers = db.list_transfers()?;
            println!(
                "{:<5} {:<30} {:<15} {:<10} {:<20}",
                "ID", "Path", "IP", "Status", "Created At"
            );
            for t in transfers {
                println!(
                    "{:<5} {:<30} {:<15} {:<10} {:<20}",
                    t.id, t.path, t.ip, t.status, t.created_at
                );
            }
        }
        Commands::Resume { id, exclude } => {
            let transfer = db.get_transfer(id)?;

            // Determine exclude patterns
            // If provided in CLI -> use them and update DB
            // If not provided -> use from DB
            let mut final_excludes = exclude;
            if !final_excludes.is_empty() {
                db.update_excludes(id, serde_json::to_string(&final_excludes)?)?;
                println!("Updated exclude patterns: {:?}", final_excludes);
            } else if let Some(json) = transfer.exclude_patterns {
                final_excludes = serde_json::from_str(&json).unwrap_or_default();
                if !final_excludes.is_empty() {
                    println!("Using exclude patterns from history: {:?}", final_excludes);
                }
            }

            let log = db::TransferLog::new(id)?;
            let path = std::path::PathBuf::from(transfer.path);

            if !transfer.listing_complete {
                println!("Listing was incomplete. Resuming scan...");
                client::scan_files(path.clone(), &log, &final_excludes).await?;
                db.set_listing_complete(id, true)?;
            } else {
                println!("Listing complete. Checking pending files...");
            }

            match client::send_pending_files(
                path,
                transfer.ip,
                transfer.port,
                &log,
                &final_excludes,
            )
            .await
            {
                Ok(_) => {
                    db.update_status(id, "Completed")?;
                    println!("Transfer resumed and completed.");
                }
                Err(e) => {
                    // Keep status properly? status is just string.
                    eprintln!("\nTransfer interrupted/failed: {}", e);
                }
            }
        }
        Commands::Restart { id, exclude } => {
            let transfer = db.get_transfer(id)?;
            println!("Restarting transfer ID: {}", id);

            // Same logic as Resume
            let mut final_excludes = exclude;
            if !final_excludes.is_empty() {
                db.update_excludes(id, serde_json::to_string(&final_excludes)?)?;
                println!("Updated exclude patterns: {:?}", final_excludes);
            } else if let Some(json) = transfer.exclude_patterns {
                final_excludes = serde_json::from_str(&json).unwrap_or_default();
                if !final_excludes.is_empty() {
                    println!("Using exclude patterns from history: {:?}", final_excludes);
                }
            }

            let log = db::TransferLog::new(id)?;
            log.reset()?;
            db.set_listing_complete(id, false)?;
            db.update_status(id, "Pending")?;

            let path = std::path::PathBuf::from(transfer.path);

            client::scan_files(path.clone(), &log, &final_excludes).await?;
            db.set_listing_complete(id, true)?;

            match client::send_pending_files(
                path,
                transfer.ip,
                transfer.port,
                &log,
                &final_excludes,
            )
            .await
            {
                Ok(_) => {
                    db.update_status(id, "Completed")?;
                    println!("Transfer restarted and completed.");
                }
                Err(e) => {
                    db.update_status(id, "Failed")?;
                    eprintln!("\nTransfer failed: {}", e);
                }
            }
        }
        Commands::Remove { id } => {
            if db.get_transfer(id).is_err() {
                eprintln!("Transfer ID {} not found.", id);
                return Ok(());
            }

            print!(
                "Are you sure you want to delete transfer ID {}? This will remove the history log. (y/N): ",
                id
            );
            use std::io::Write;
            std::io::stdout().flush()?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            if input.trim().eq_ignore_ascii_case("y") {
                db.delete_transfer(id)?;

                let log_file = format!("send_history_{}.db", id);
                let log_path = std::path::Path::new(&log_file);
                if log_path.exists() {
                    std::fs::remove_file(log_path)?;
                    let _ = std::fs::remove_file(format!("send_history_{}.db-wal", id));
                    let _ = std::fs::remove_file(format!("send_history_{}.db-shm", id));
                }
                println!("Transfer ID {} removed.", id);
            } else {
                println!("Operation cancelled.");
            }
        }
    }

    Ok(())
}
