use crate::db::TransferLog;
use crate::protocol::{FileMetadata, ServerResponse};
use anyhow::{Result, anyhow};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::net::TcpStream;
use walkdir::WalkDir;

use glob::Pattern;

pub async fn scan_files(
    source_path: PathBuf,
    log: &TransferLog,
    exclude_patterns: &[String],
) -> Result<()> {
    println!("Scanning files (Excludes: {:?})...", exclude_patterns);
    let walker = WalkDir::new(&source_path);
    let mut count = 0;

    let patterns: Vec<Pattern> = exclude_patterns
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect();

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        let _should_process = true; // Process all, log everything

        // Relative path logic
        let relative_path_str = if source_path.is_file() {
            path.file_name().unwrap().to_string_lossy().to_string()
        } else {
            let root = source_path.parent().unwrap_or(Path::new("."));
            path.strip_prefix(root)?.to_string_lossy().to_string()
        };

        // Normalize path separators
        let relative_path_clean = relative_path_str.replace("\\", "/");

        // Check exclude patterns
        if patterns.iter().any(|p| p.matches(&relative_path_clean)) {
            // println!("Excluded: {}", relative_path_clean);
            continue;
        }

        // Also check if any parent directory is excluded for safer skipping?
        // WalkDir usually recurses. If we exclude "node_modules", we want to skip everything inside it.
        // Glob matching "node_modules" usually only matches the directory itself if relative_path_clean is exactly "node_modules".
        // It won't match "node_modules/foo.js".
        // Users often expect "node_modules" to exclude recursive.
        // But glob behavior matches string.
        // If user provides "node_modules" and we have "project/node_modules/file.txt".
        // We probably want to support standard glob (git-like) if possible, but simplicity first.
        // If user says "**/node_modules/**" it works.
        // But let's assume if it matches, we skip.

        let metadata = fs::metadata(path).await?;
        let is_dir = metadata.is_dir();
        let size = metadata.len();

        log.add_file(&relative_path_clean, size, is_dir)?;
        count += 1;

        if count % 100 == 0 {
            print!("\rScanned: {} items", count);
            std::io::stdout().flush()?;
        }
    }
    println!("\rScanned: {} items. Scan complete.", count);
    Ok(())
}

pub async fn send_pending_files(
    source_path: PathBuf,
    ip: String,
    port: u16,
    log: &TransferLog,
    exclude_patterns: &[String],
) -> Result<()> {
    // Connect to server
    let addr = format!("{}:{}", ip, port);
    println!("Connecting to {}...", addr);
    let mut socket = TcpStream::connect(&addr).await?;
    socket.set_nodelay(true)?; // Disable Nagle's algorithm for lower latency
    println!("Connected.");

    let pending_files = log.get_pending_files()?;
    if pending_files.is_empty() {
        println!("No pending files to send.");
        return Ok(());
    }

    // Compile patterns for filtering
    let patterns: Vec<Pattern> = exclude_patterns
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect();

    let mut total_files_sent = log.count_total()? - log.count_pending()?;
    let mut total_skipped = log.count_skipped()?;
    let total_bytes_sent_from_log = log.get_total_sent_bytes()?; // Total bytes sent from previous sessions
    let mut current_total_bytes_sent = 0u64; // Bytes sent in this session
    let mut last_update = Instant::now();
    let start_time = Instant::now();
    let update_interval = Duration::from_millis(300);

    // We need total pending size for ETA
    let mut total_pending_size: u64 = pending_files.iter().map(|f| f.size).sum();
    let mut session_bytes_sent = 0u64;

    // Initial status
    print!(
        "\rSending: Files: {}, Skipped: {}, Size: 0 B, ETA: --:--",
        total_files_sent, total_skipped
    );
    std::io::stdout().flush()?;

    for record in pending_files {
        // Construct absolute path
        // source_path might be a file or folder.
        // We recorded relative path from `source_path.parent()` (if dir) or filename (if file).
        // If source_path was "d:/Projects/send", parent is "d:/Projects". relative is "send/..."
        // So absolute path = parent.join(relative)
        let root = source_path.parent().unwrap_or(Path::new("."));
        let file_path = root.join(&record.relative_path); // relative_path is DB path (forward slashes). Windows handles mixed? best to ensure.
        // On Windows join works fine with forward slash usually, but let's check.

        // Check if excluded
        if patterns.iter().any(|p| p.matches(&record.relative_path)) {
            log.mark_skipped(&record.relative_path)?;
            total_skipped += 1;
            total_pending_size = total_pending_size.saturating_sub(record.size);
            continue;
        }

        if !file_path.exists() {
            eprintln!("\nWarning: File not found: {:?}, skipping.", file_path);
            // This file was pending, but now it's gone. We should remove its size from total_pending_size.
            total_pending_size = total_pending_size.saturating_sub(record.size);
            continue; // Should we mark as failed?
        }

        let is_dir = record.is_dir;
        let size = record.size;
        let relative_path_clean = record.relative_path;

        let meta = FileMetadata {
            relative_path: relative_path_clean.clone(),
            size,
            is_dir,
        };

        // Update UI loop
        if last_update.elapsed() >= update_interval {
            let elapsed = start_time.elapsed().as_secs_f64();
            let rate = if elapsed > 0.0 {
                session_bytes_sent as f64 / elapsed
            } else {
                0.0
            };
            let remaining_bytes = total_pending_size.saturating_sub(session_bytes_sent);
            let eta_seconds = if rate > 0.0 {
                remaining_bytes as f64 / rate
            } else {
                0.0
            };

            let eta_str = if eta_seconds > 3600.0 {
                format!(
                    "{:.0}h {:.0}m",
                    eta_seconds / 3600.0,
                    (eta_seconds % 3600.0) / 60.0
                )
            } else if eta_seconds > 60.0 {
                format!("{:.0}m {:.0}s", eta_seconds / 60.0, eta_seconds % 60.0)
            } else {
                format!("{:.0}s", eta_seconds)
            };

            print!(
                "\rSending: Files: {}, Skipped: {}, Size: {} | ETA: {} | Current: {:.30}               ",
                total_files_sent,
                total_skipped,
                format_size(total_bytes_sent_from_log + current_total_bytes_sent),
                eta_str,
                relative_path_clean
            );
            std::io::stdout().flush()?;
            last_update = Instant::now();
        }

        // Send metadata
        let json = serde_json::to_vec(&meta)?;
        let len = (json.len() as u32).to_be_bytes();
        socket.write_all(&len).await?;
        socket.write_all(&json).await?;

        // Wait for response
        let mut len_buf = [0u8; 4];
        if socket.read_exact(&mut len_buf).await.is_err() {
            return Err(anyhow!("Connection closed by server"));
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut resp_buf = vec![0u8; len];
        socket.read_exact(&mut resp_buf).await?;

        let response: ServerResponse = serde_json::from_slice(&resp_buf)?;

        match response {
            ServerResponse::Skip => {
                // Server says skip
                if !is_dir {
                    log.mark_skipped(&relative_path_clean)?;
                    total_skipped += 1;
                    // This file was pending, but now it's skipped. We should remove its size from total_pending_size.
                    total_pending_size = total_pending_size.saturating_sub(size);
                } else {
                    // directory technically "sent"/processed
                    log.mark_sent(&relative_path_clean)?;
                    // Directories don't have size, so no need to adjust total_pending_size for them.
                }
            }
            ServerResponse::Send => {
                if !is_dir {
                    let mut file = File::open(&file_path).await?;
                    let current_size = file.metadata().await?.len();
                    if current_size != size {
                        return Err(anyhow!("File changed: {}", relative_path_clean));
                    }

                    // Custom copy loop with progress
                    // Increased buffer size to 1MB
                    let mut buf = vec![0u8; 1024 * 1024];
                    let mut remaining = size;
                    let mut file_sent = 0;

                    loop {
                        let to_read = std::cmp::min(buf.len() as u64, remaining) as usize;
                        if to_read == 0 {
                            break;
                        }

                        let n = file.read_exact(&mut buf[..to_read]).await?;

                        socket.write_all(&buf[..n]).await?;

                        remaining -= n as u64;
                        current_total_bytes_sent += n as u64;
                        session_bytes_sent += n as u64;
                        file_sent += n as u64;

                        if last_update.elapsed() >= update_interval {
                            let elapsed = start_time.elapsed().as_secs_f64();
                            let rate = if elapsed > 0.0 {
                                session_bytes_sent as f64 / elapsed
                            } else {
                                0.0
                            };
                            let remaining_bytes =
                                total_pending_size.saturating_sub(session_bytes_sent);
                            let eta_seconds = if rate > 0.0 {
                                remaining_bytes as f64 / rate
                            } else {
                                0.0
                            };

                            let eta_str = if eta_seconds > 3600.0 {
                                format!(
                                    "{:.0}h {:.0}m",
                                    eta_seconds / 3600.0,
                                    (eta_seconds % 3600.0) / 60.0
                                )
                            } else if eta_seconds > 60.0 {
                                format!("{:.0}m {:.0}s", eta_seconds / 60.0, eta_seconds % 60.0)
                            } else {
                                format!("{:.0}s", eta_seconds)
                            };

                            print!(
                                "\rSending: Files: {}, Skipped: {}, Size: {} | ETA: {} | Current: {:.30}               ",
                                total_files_sent,
                                total_skipped,
                                format_size(total_bytes_sent_from_log + current_total_bytes_sent),
                                eta_str,
                                relative_path_clean
                            );
                            std::io::stdout().flush()?;
                            last_update = Instant::now();
                        }
                    }

                    if file_sent != size {
                        return Err(anyhow!("Incomplete transfer: {}", relative_path_clean));
                    }

                    total_files_sent += 1;
                    log.mark_sent(&relative_path_clean)?;
                } else {
                    log.mark_sent(&relative_path_clean)?;
                }
            }
            ServerResponse::Resume { offset } => {
                if !is_dir {
                    let mut file = File::open(&file_path).await?;
                    file.seek(tokio::io::SeekFrom::Start(offset)).await?;

                    // Increased buffer size to 1MB
                    let mut buf = vec![0u8; 1024 * 1024];
                    let mut remaining = size - offset; // Send remainder

                    loop {
                        let to_read = std::cmp::min(buf.len() as u64, remaining) as usize;
                        if to_read == 0 {
                            break;
                        }

                        let n = file.read(&mut buf[..to_read]).await?;
                        if n == 0 {
                            break;
                        } // EOF

                        socket.write_all(&buf[..n]).await?;

                        remaining -= n as u64;
                        current_total_bytes_sent += n as u64;
                        session_bytes_sent += n as u64;

                        if last_update.elapsed() >= update_interval {
                            // ... same UI update code ...
                            let elapsed = start_time.elapsed().as_secs_f64();
                            let rate = if elapsed > 0.0 {
                                session_bytes_sent as f64 / elapsed
                            } else {
                                0.0
                            };
                            let remaining_bytes =
                                total_pending_size.saturating_sub(session_bytes_sent);
                            let eta_seconds = if rate > 0.0 {
                                remaining_bytes as f64 / rate
                            } else {
                                0.0
                            };

                            let eta_str = if eta_seconds > 3600.0 {
                                format!(
                                    "{:.0}h {:.0}m",
                                    eta_seconds / 3600.0,
                                    (eta_seconds % 3600.0) / 60.0
                                )
                            } else if eta_seconds > 60.0 {
                                format!("{:.0}m {:.0}s", eta_seconds / 60.0, eta_seconds % 60.0)
                            } else {
                                format!("{:.0}s", eta_seconds)
                            };

                            print!(
                                "\rSending: Files: {}, Skipped: {}, Size: {} | ETA: {} | Current: {:.30}               ",
                                total_files_sent,
                                total_skipped,
                                format_size(total_bytes_sent_from_log + current_total_bytes_sent),
                                eta_str,
                                relative_path_clean
                            );
                            std::io::stdout().flush()?;
                            last_update = Instant::now();
                        }
                    }

                    total_files_sent += 1;
                    log.mark_sent(&relative_path_clean)?;
                } else {
                    log.mark_sent(&relative_path_clean)?;
                }
            }
            ServerResponse::Error { message } => {
                return Err(anyhow!("Server error: {}", message));
            }
        }
    }

    // Final update
    println!(
        "\rDone! Total Files: {}, Skipped: {}, Total Size: {}                                    ",
        total_files_sent,
        total_skipped,
        format_size(total_bytes_sent_from_log + current_total_bytes_sent)
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
