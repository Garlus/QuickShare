use crate::protocol::*;
use crate::transfer::connection::Connection;
use crate::transfer::encryption::CryptoContext;
use crate::ffi;
use anyhow::{Result, anyhow};
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use rand::RngCore;

pub struct FileReceiver {
    connection: Connection,
    #[allow(dead_code)]
    crypto: CryptoContext,
    save_dir: PathBuf,
    device_name: String,
    file_number: i32,
}

impl FileReceiver {
    pub fn new(connection: Connection, crypto: CryptoContext, save_dir: PathBuf, device_name: String, file_number: i32) -> Self {
        FileReceiver { connection, crypto, save_dir, device_name, file_number }
    }

    pub async fn receive_file(&mut self) -> Result<()> {
        // 1. Receive peer's PairedKeyEncryption frame
        let _peer_paired_enc: SharingFrame = self.connection.recv_secure().await?;
        info!("Received PairedKeyEncryption frame");

        // 2. Send PairedKeyEncryption frame back
        let mut signed_data = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut signed_data);
        let mut secret_id_hash = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret_id_hash);

        let paired_enc = SharingFrame::new_paired_key_encryption(signed_data, secret_id_hash);
        self.connection.send_secure(&paired_enc).await?;
        info!("Sent PairedKeyEncryption frame");

        // 3. Receive peer's PairedKeyResult frame
        let _peer_paired_result: SharingFrame = self.connection.recv_secure().await?;
        info!("Received PairedKeyResult frame");

        // 4. Send PairedKeyResult (UNABLE)
        let result = SharingFrame::new_paired_key_result(PairedKeyResultStatus::Unable);
        self.connection.send_secure(&result).await?;
        info!("Sent PairedKeyResult (UNABLE)");

        // 5. Receive Introduction frame (sharing layer, via SecureMessage)
        let intro_frame: SharingFrame = self.connection.recv_secure().await?;
        let v1 = intro_frame.v1.as_ref()
            .ok_or_else(|| anyhow!("Introduction missing v1"))?;

        let file_meta = v1.introduction.as_ref()
            .and_then(|i| i.file_metadata.first())
            .ok_or_else(|| anyhow!("No file metadata in Introduction"))?;

        let file_name = file_meta.name.as_deref()
            .ok_or_else(|| anyhow!("Missing file name"))?;
        let file_size = file_meta.size
            .ok_or_else(|| anyhow!("Missing file size"))?;
        let _payload_id = file_meta.payload_id.unwrap_or(0);

        info!("Incoming file: {} ({} bytes) from {}", file_name, file_size, self.device_name);

        // 6. Ask user to accept
        let (_request_id, rx) = ffi::register_incoming_transfer(
            &self.device_name,
            file_name,
            file_size,
            self.file_number,
        );
        let accepted = rx.await.unwrap_or(false);
        if !accepted {
            warn!("User denied file transfer from {}", self.device_name);
            let reject = SharingFrame::new_connection_response(false);
            self.connection.send_secure(&reject).await.ok();
            return Err(anyhow!("Transfer denied by user"));
        }

        // Send ConnectionResponse(ACCEPT)
        let conn_resp = SharingFrame::new_connection_response(true);
        self.connection.send_secure(&conn_resp).await?;
        info!("Sent ConnectionResponse (ACCEPT)");

        // 5. Receive PayloadTransfer header (OfflineFrame via SecureMessage)
        let transfer_frame: OfflineFrame = self.connection.recv_secure().await?;
        let transfer_v1 = transfer_frame.v1.as_ref()
            .ok_or_else(|| anyhow!("PayloadTransfer missing v1"))?;
        let transfer = transfer_v1.payload_transfer.as_ref()
            .ok_or_else(|| anyhow!("Missing payload_transfer"))?;
        let _header = transfer.payload_header.as_ref()
            .ok_or_else(|| anyhow!("Missing payload_header"))?;

        info!("Receiving file: {} ({} bytes)", file_name, file_size);

        let save_path = self.save_dir.join(file_name);
        let mut file = File::create(&save_path).await
            .map_err(|e| anyhow!("Failed to create file {:?}: {}", save_path, e))?;

        let mut received: i64 = 0;

        // 6. Receive PayloadChunk frames until file is complete
        loop {
            if received >= file_size {
                break;
            }

            let chunk_frame: OfflineFrame = self.connection.recv_secure().await?;
            let chunk_v1 = chunk_frame.v1.as_ref()
                .ok_or_else(|| anyhow!("Chunk missing v1"))?;

            match chunk_v1.r#type {
                Some(t) if t == OfflineFrameType::PayloadTransfer as i32 => {
                    let transfer = chunk_v1.payload_transfer.as_ref()
                        .ok_or_else(|| anyhow!("Missing payload_transfer in chunk"))?;

                    if let Some(chunk) = &transfer.payload_chunk {
                        let body = chunk.body.as_deref()
                            .ok_or_else(|| anyhow!("Missing chunk body"))?;
                        file.write_all(body).await
                            .map_err(|e| anyhow!("Failed to write chunk: {}", e))?;
                        received += body.len() as i64;
                        info!("Received {}/{} bytes for {}", received, file_size, file_name);
                    }
                }
                Some(t) if t == OfflineFrameType::Disconnection as i32 => {
                    warn!("Remote disconnected mid-transfer");
                    break;
                }
                _ => {
                    warn!("Unexpected frame type during transfer");
                }
            }
        }

        info!("File received: {} ({} bytes)", file_name, received);
        Ok(())
    }
}
