use crate::protocol::{FrameType};
use crate::transfer::connection::Connection;
use crate::transfer::encryption::CryptoContext;
use anyhow::{Result, anyhow};
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};


pub struct FileReceiver {
    connection: Connection,
    crypto: CryptoContext,
    save_dir: PathBuf,
}

impl FileReceiver {
    pub fn new(connection: Connection, crypto: CryptoContext, save_dir: PathBuf) -> Self {
        FileReceiver { connection, crypto, save_dir }
    }

    /// Listen for an incoming file transfer and save it.
    pub async fn receive_file(&mut self) -> Result<()> {
        // Wait for payload transfer header
        let (frame, _) = self.connection.recv_frame().await?;
        let v1 = frame.v1.as_ref()
            .ok_or_else(|| anyhow!("Missing V1 frame"))?;

        match v1.r#type {
            Some(t) if t == FrameType::PayloadTransfer as i32 => {
                let transfer = v1.payload_transfer.as_ref()
                    .ok_or_else(|| anyhow!("Missing payload transfer"))?;

                let file_info = transfer.file_info.as_ref()
                    .ok_or_else(|| anyhow!("Missing file info"))?;

                let file_name = file_info.file_name.as_deref()
                    .ok_or_else(|| anyhow!("Missing file name"))?;
                let file_size = file_info.file_size
                    .ok_or_else(|| anyhow!("Missing file size"))?;
                let _payload_id = transfer.payload_id.as_ref()
                    .ok_or_else(|| anyhow!("Missing payload ID"))?;

                let save_path = self.save_dir.join(file_name);
                info!("Receiving file: {} ({} bytes) -> {:?}", file_name, file_size, save_path);

                let mut file = File::create(&save_path).await
                    .map_err(|e| anyhow!("Failed to create file {:?}: {}", save_path, e))?;

                let mut received: i64 = 0;
                let mut file_buffer = Vec::new();

                loop {
                    if received >= file_size {
                        break;
                    }

                    let (chunk_frame, _) = self.connection.recv_frame().await?;
                    let chunk_v1 = chunk_frame.v1.as_ref()
                        .ok_or_else(|| anyhow!("Missing V1 in chunk"))?;

                    match chunk_v1.r#type {
                        Some(t) if t == FrameType::PayloadChunk as i32 => {
                            let chunk = chunk_v1.payload_chunk.as_ref()
                                .ok_or_else(|| anyhow!("Missing payload chunk"))?;

                            let chunk_data = chunk.chunk.as_ref()
                                .ok_or_else(|| anyhow!("Missing chunk data"))?;

                            // First 12 bytes are the nonce, rest is ciphertext
                            if chunk_data.len() < 12 {
                                return Err(anyhow!("Chunk too small"));
                            }
                            let nonce = &chunk_data[..12];
                            let ciphertext = &chunk_data[12..];

                            let plaintext = self.crypto.decrypt(nonce, ciphertext).await?;
                            file_buffer.extend_from_slice(&plaintext);
                            received += plaintext.len() as i64;

                            info!("Received {}/{} bytes for {}",
                                  received, file_size, file_name);
                        }
                        Some(t) if t == FrameType::Disconnection as i32 => {
                            warn!("Remote disconnected mid-transfer");
                            break;
                        }
                        _ => {
                            warn!("Unexpected frame type during transfer");
                        }
                    }
                }

                file.write_all(&file_buffer).await
                    .map_err(|e| anyhow!("Failed to write file: {}", e))?;

                info!("File received: {} ({} bytes)", file_name, received);
                Ok(())
            }
            Some(t) if t == FrameType::Disconnection as i32 => {
                Err(anyhow!("Remote disconnected"))
            }
            _ => {
                Err(anyhow!("Expected payload transfer frame"))
            }
        }
    }
}
