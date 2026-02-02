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
        Commands::Push { path, ip, port } => {
            let abs_path = std::fs::canonicalize(&path).unwrap_or(path.clone());
            let id = db.add_transfer(&abs_path.to_string_lossy(), &ip, port)?;
            println!("Transfer started with ID: {}", id);

            let log = db::TransferLog::new(id)?;
            client::scan_files(abs_path.clone(), &log).await?;
            db.set_listing_complete(id, true)?;

            match client::send_pending_files(abs_path, ip, port, &log).await {
                Ok(_) => {
                    db.update_status(id, "Completed")?;
                    println!("Transfer completed successfully.");
                }
                Err(e) => {
                    db.update_status(id, "Failed")?;
                    eprintln!("Transfer failed: {}", e);
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
        Commands::Resume { id } => {
            let transfer = db.get_transfer(id)?;
            let log = db::TransferLog::new(id)?;
            let path = std::path::PathBuf::from(transfer.path);

            if !transfer.listing_complete {
                println!("Listing was incomplete. Resuming scan...");
                client::scan_files(path.clone(), &log).await?;
                db.set_listing_complete(id, true)?;
            } else {
                println!("Listing complete. Checking pending files...");
            }

            match client::send_pending_files(path, transfer.ip, transfer.port, &log).await {
                Ok(_) => {
                    db.update_status(id, "Completed")?;
                    println!("Transfer resumed and completed.");
                }
                Err(e) => {
                    // Keep status properly? status is just string.
                    eprintln!("Transfer interrupted/failed: {}", e);
                }
            }
        }
        Commands::Restart { id } => {
            let transfer = db.get_transfer(id)?;
            println!("Restarting transfer ID: {}", id);

            let log = db::TransferLog::new(id)?;
            log.reset()?;
            db.set_listing_complete(id, false)?;
            db.update_status(id, "Pending")?;

            let path = std::path::PathBuf::from(transfer.path);

            client::scan_files(path.clone(), &log).await?;
            db.set_listing_complete(id, true)?;

            match client::send_pending_files(path, transfer.ip, transfer.port, &log).await {
                Ok(_) => {
                    db.update_status(id, "Completed")?;
                    println!("Transfer restarted and completed.");
                }
                Err(e) => {
                    db.update_status(id, "Failed")?;
                    eprintln!("Transfer failed: {}", e);
                }
            }
        }
    }

    Ok(())
}
