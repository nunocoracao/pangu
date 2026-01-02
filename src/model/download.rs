//! Model downloading from Hugging Face with progress tracking and resume support

use std::path::Path;
use std::time::Instant;
use thiserror::Error;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use futures::StreamExt;

/// Download error types
#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Missing content length header")]
    MissingContentLength,
    #[error("Server doesn't support resume")]
    ResumeNotSupported,
    #[error("Download cancelled")]
    Cancelled,
}

/// Download progress information
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    /// Bytes downloaded so far
    pub downloaded: u64,
    /// Total file size in bytes
    pub total: u64,
    /// Download speed in bytes per second
    pub speed: f64,
}

impl DownloadProgress {
    /// Get progress as a percentage (0.0 to 1.0)
    pub fn percentage(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.downloaded as f64 / self.total as f64
        }
    }

    /// Format downloaded/total as human readable string
    pub fn size_string(&self) -> String {
        format!("{} / {}", format_bytes(self.downloaded), format_bytes(self.total))
    }

    /// Format speed as human readable string
    pub fn speed_string(&self) -> String {
        format!("{}/s", format_bytes(self.speed as u64))
    }
}

/// Model downloader with progress tracking
pub struct ModelDownloader {
    client: reqwest::Client,
}

impl ModelDownloader {
    /// Create a new downloader
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("pangu/0.1.0")
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Download a file from URL to destination with progress reporting
    /// Supports resuming interrupted downloads
    pub async fn download(
        &self,
        url: &str,
        dest: &Path,
        progress_tx: mpsc::UnboundedSender<DownloadProgress>,
    ) -> Result<(), DownloadError> {
        // Create parent directories if needed
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Check for existing partial download
        let temp_path = dest.with_extension("tmp");
        let existing_size = if temp_path.exists() {
            tokio::fs::metadata(&temp_path).await?.len()
        } else {
            0
        };

        // Build request - add Range header if we have a partial file
        let mut request = self.client.get(url);
        if existing_size > 0 {
            tracing::info!("Attempting to resume from {}", format_bytes(existing_size));
            request = request.header("Range", format!("bytes={}-", existing_size));
        }

        // Start the request
        let response = request.send().await?.error_for_status()?;
        let status = response.status();

        // Determine total size and starting point based on response
        let (total, mut downloaded, mut file) = if status == reqwest::StatusCode::PARTIAL_CONTENT {
            // Server supports resume - parse Content-Range header
            // Format: "bytes start-end/total"
            let content_range = response
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.split('/').last())
                .and_then(|s| s.parse::<u64>().ok());

            let total = content_range.ok_or(DownloadError::MissingContentLength)?;

            tracing::info!("Resuming download from {} ({:.1}%)",
                format_bytes(existing_size),
                (existing_size as f64 / total as f64) * 100.0
            );

            // Open file in append mode
            let file = OpenOptions::new()
                .append(true)
                .open(&temp_path)
                .await?;

            (total, existing_size, file)
        } else {
            // Fresh download (either no partial file or server doesn't support resume)
            let total = response
                .content_length()
                .ok_or(DownloadError::MissingContentLength)?;

            if existing_size > 0 {
                tracing::warn!("Server doesn't support resume, starting fresh download");
            }

            let file = File::create(&temp_path).await?;
            (total, 0u64, file)
        };

        // Send initial progress
        let _ = progress_tx.send(DownloadProgress {
            downloaded,
            total,
            speed: 0.0,
        });

        // Stream the response body
        let mut stream = response.bytes_stream();
        let start_time = Instant::now();
        let mut last_progress_time = Instant::now();
        let mut last_downloaded: u64 = downloaded;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            // Calculate speed and send progress update (throttled to ~10 updates/sec)
            let now = Instant::now();
            let elapsed_since_last = now.duration_since(last_progress_time).as_secs_f64();

            if elapsed_since_last >= 0.1 || downloaded == total {
                let bytes_since_last = downloaded - last_downloaded;
                let speed = if elapsed_since_last > 0.0 {
                    bytes_since_last as f64 / elapsed_since_last
                } else {
                    0.0
                };

                let progress = DownloadProgress {
                    downloaded,
                    total,
                    speed,
                };

                // Ignore send errors (receiver might be gone)
                let _ = progress_tx.send(progress);

                last_progress_time = now;
                last_downloaded = downloaded;
            }
        }

        // Flush and close the file
        file.flush().await?;
        drop(file);

        // Atomic rename from temp to final path
        tokio::fs::rename(&temp_path, dest).await?;

        tracing::info!(
            "Download complete: {} in {:.1}s",
            format_bytes(total),
            start_time.elapsed().as_secs_f64()
        );

        Ok(())
    }
}

impl Default for ModelDownloader {
    fn default() -> Self {
        Self::new()
    }
}

/// Format bytes as human-readable string (e.g., "1.5 GB")
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
