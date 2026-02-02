use crate::protocol::{FileMetadata, ServerResponse};
use anyhow::Result;
use std::path::{Component, PathBuf};
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub async fn run_server(base_path: PathBuf, port: u16) -> Result<()> {
    if !base_path.exists() {
        fs::create_dir_all(&base_path).await?;
    }

    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    println!(
        "Resuming server listening on {} saving to {:?}",
        addr, base_path
    );

    loop {
        let (socket, _) = listener.accept().await?;
        let base_path = base_path.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, base_path).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
}

async fn handle_connection(mut socket: TcpStream, base_path: PathBuf) -> Result<()> {
    socket.set_nodelay(true)?; // Optimize latency
    let mut total_files_recvd = 0u64;
    let mut total_skipped = 0u64;
    let mut total_bytes_recvd = 0u64;
    let mut last_update = std::time::Instant::now();
    let update_interval = std::time::Duration::from_millis(300);

    // Initial status
    print!("\rReceiving: Files: 0, Skipped: 0, Size: 0 B");
    let _ = std::io::Write::flush(&mut std::io::stdout());

    loop {
        // Read metadata length
        let mut len_buf = [0u8; 4];
        if socket.read_exact(&mut len_buf).await.is_err() {
            break; // Client disconnected
        }
        let len = u32::from_be_bytes(len_buf) as usize;

        // Read metadata
        let mut meta_buf = vec![0u8; len];
        socket.read_exact(&mut meta_buf).await?;
        let metadata: FileMetadata = serde_json::from_slice(&meta_buf)?;

        let relative_path = PathBuf::from(&metadata.relative_path);
        if relative_path
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            eprintln!(
                "Security warning: Attempt to write outside base path: {:?}",
                metadata.relative_path
            );
            send_response(
                &mut socket,
                ServerResponse::Error {
                    message: "Invalid path".into(),
                },
            )
            .await?;
            continue;
        }

        // println!("Receiving: {:?}", metadata.relative_path); // Removed to avoid interfering with progress bar

        let target_path = base_path.join(&relative_path);

        if metadata.is_dir {
            fs::create_dir_all(&target_path).await?;
            send_response(&mut socket, ServerResponse::Skip).await?;
            continue;
        }

        // Check if file exists AND matches size
        let skip = if target_path.exists() {
            let meta = fs::metadata(&target_path).await?;
            meta.len() == metadata.size
        } else {
            false
        };

        if skip {
            send_response(&mut socket, ServerResponse::Skip).await?;
            total_skipped += 1;
            // println!("Skipping existing: {:?}", metadata.relative_path);
            continue;
        }

        let temp_path = target_path.with_file_name(format!(
            "{}.tmp",
            target_path.file_name().unwrap().to_string_lossy()
        ));
        let mut offset = 0;

        let mut file = if temp_path.exists() {
            let f = File::options().append(true).open(&temp_path).await?;
            offset = f.metadata().await?.len();
            if offset > metadata.size {
                // Invalid state, start over
                let f = File::create(&temp_path).await?;
                offset = 0;
                f
            } else {
                f
            }
        } else {
            if let Some(parent) = temp_path.parent() {
                fs::create_dir_all(parent).await?;
            }
            File::create(&temp_path).await?
        };

        if offset > 0 && offset < metadata.size {
            send_response(&mut socket, ServerResponse::Resume { offset }).await?;
            // println!("Resuming from: {}", offset);
        } else if offset >= metadata.size {
            // Already downloaded fully in temp?
            file.shutdown().await?;
            fs::rename(&temp_path, &target_path).await?;
            send_response(&mut socket, ServerResponse::Skip).await?;
            total_skipped += 1;
            // println!("Restored from temp: {:?}", metadata.relative_path);
            continue;
        } else {
            send_response(&mut socket, ServerResponse::Send).await?;
        }

        // Receive File content with Progress Update
        let remaining = metadata.size - offset;
        let mut take = (&mut socket).take(remaining);

        // Custom copy loop for progress
        // Increased buffer size to 1MB
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            let n = take.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).await?;

            total_bytes_recvd += n as u64;

            if last_update.elapsed() >= update_interval {
                print!(
                    "\rReceiving: Files: {}, Skipped: {}, Size: {} | Current: {:.30}               ",
                    total_files_recvd,
                    total_skipped,
                    format_size(total_bytes_recvd),
                    metadata.relative_path
                );
                let _ = std::io::Write::flush(&mut std::io::stdout());
                last_update = std::time::Instant::now();
            }
        }

        file.flush().await?;
        fs::rename(&temp_path, &target_path).await?;

        total_files_recvd += 1;
        // println!("Finished: {:?}", metadata.relative_path);
    }
    println!(
        "\rDone! Total Files: {}, Skipped: {}, Total Size: {}                                    ",
        total_files_recvd,
        total_skipped,
        format_size(total_bytes_recvd)
    );
    Ok(())
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

async fn send_response(socket: &mut TcpStream, resp: ServerResponse) -> Result<()> {
    let json = serde_json::to_vec(&resp)?;
    let len = (json.len() as u32).to_be_bytes();
    socket.write_all(&len).await?;
    socket.write_all(&json).await?;
    Ok(())
}
