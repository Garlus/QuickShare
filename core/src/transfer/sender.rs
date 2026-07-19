use crate::protocol::{Frame, V1Frame};
use crate::transfer::connection::Connection;
use crate::transfer::encryption::CryptoContext;
use anyhow::{Result, anyhow};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tracing::info;

const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks

pub struct FileSender {
    connection: Connection,
    crypto: CryptoContext,
}

impl FileSender {
    pub fn new(connection: Connection, crypto: CryptoContext) -> Self {
        FileSender { connection, crypto }
    }

    /// Send a single file to the connected device.
    pub async fn send_file(&mut self, file_path: &Path) -> Result<()> {
        let file_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("Invalid file name"))?
            .to_string();

        let metadata = tokio::fs::metadata(file_path).await
            .map_err(|e| anyhow!("Failed to read file metadata: {}", e))?;
        let file_size = metadata.len() as i64;

        let mime = mime_guess2::from_path(file_path)
            .first_or_octet_stream()
            .to_string();

        info!("Sending file: {} ({} bytes, {})", file_name, file_size, mime);

        // Send payload transfer header
        let payload_id = super::encryption::generate_payload_id();
        let transfer = V1Frame::new_payload_transfer(
            payload_id.clone(),
            file_name.clone(),
            file_size,
            mime,
        );
        self.connection.send_frame(&Frame::new_v1(transfer)).await?;

        // Open file and stream chunks
        let mut file = File::open(file_path).await
            .map_err(|e| anyhow!("Failed to open file: {}", e))?;

        let mut offset: i64 = 0;
        let mut buffer = vec![0u8; CHUNK_SIZE];

        loop {
            let bytes_read = file.read(&mut buffer).await
                .map_err(|e| anyhow!("Failed to read file: {}", e))?;
            if bytes_read == 0 {
                break;
            }

            let chunk_data = &buffer[..bytes_read];

            // Encrypt the chunk
            let (nonce, ciphertext) = self.crypto.encrypt(chunk_data).await?;

            // Build payload chunk frame
            let chunk = V1Frame::new_payload_chunk(
                payload_id.clone(),
                offset,
                [&nonce[..], &ciphertext[..]].concat(),
            );
            self.connection.send_frame(&Frame::new_v1(chunk)).await?;

            offset += bytes_read as i64;
            info!("Sent {}/{} bytes for {}", offset, file_size, file_name);
        }

        info!("File sent: {}", file_name);
        Ok(())
    }

    /// Send multiple files sequentially.
    pub async fn send_files(&mut self, file_paths: &[&Path]) -> Result<()> {
        for path in file_paths {
            self.send_file(path).await?;
        }
        Ok(())
    }
}
