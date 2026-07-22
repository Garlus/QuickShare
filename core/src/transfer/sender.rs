use crate::protocol::*;
use crate::transfer::connection::Connection;
use crate::transfer::encryption::CryptoContext;
use anyhow::{Result, anyhow};
use rand::RngCore;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tracing::info;

pub type ProgressCallback = fn(i64, i64, &str);

static PROGRESS_CB: std::sync::OnceLock<ProgressCallback> = std::sync::OnceLock::new();

pub fn set_progress_callback(cb: ProgressCallback) {
    let _ = PROGRESS_CB.set(cb);
}

const CHUNK_SIZE: usize = 64 * 1024;

pub struct FileSender {
    connection: Connection,
    #[allow(dead_code)]
    crypto: CryptoContext,
}

impl FileSender {
    pub fn new(connection: Connection, crypto: CryptoContext) -> Self {
        FileSender { connection, crypto }
    }

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

        let payload_id_bytes = super::encryption::generate_payload_id();
        let payload_id = i64::from_be_bytes(payload_id_bytes[..8].try_into().unwrap());

        // 1. Send PairedKeyEncryption frame
        let mut signed_data = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut signed_data);
        let mut secret_id_hash = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret_id_hash);

        let paired_enc = SharingFrame::new_paired_key_encryption(signed_data, secret_id_hash);
        self.connection.send_secure(&paired_enc).await?;
        info!("Sent PairedKeyEncryption frame");

        // 2. Receive peer's PairedKeyEncryption frame
        let _peer_paired_enc: SharingFrame = self.connection.recv_secure().await?;
        info!("Received peer PairedKeyEncryption frame");

        // 3. Send PairedKeyResult (UNABLE)
        let paired_result = SharingFrame::new_paired_key_result(PairedKeyResultStatus::Unable);
        self.connection.send_secure(&paired_result).await?;
        info!("Sent PairedKeyResult frame (UNABLE)");

        // 4. Receive peer's PairedKeyResult
        let peer_paired_result: SharingFrame = self.connection.recv_secure().await?;
        if let Some(v1) = &peer_paired_result.v1 {
            if let Some(result) = &v1.paired_key_result {
                info!("Received peer PairedKeyResult frame (status={:?})", result.status);
            }
        }

        // 5. Send Introduction (sharing layer, via SecureMessage)
        let file_meta = FileMetadata {
            name: Some(file_name.clone()),
            r#type: Some(sharing_proto::file_metadata::Type::Image as i32),
            payload_id: Some(payload_id),
            size: Some(file_size),
            mime_type: Some(mime),
            id: Some(1),
        };
        let intro = SharingFrame::new_introduction(vec![file_meta]);
        self.connection.send_secure(&intro).await?;
        info!("Sent Introduction frame");

        // 6. Receive ConnectionResponse from peer (ACCEPT / REJECT)
        let conn_resp_frame: SharingFrame = self.connection.recv_secure().await?;
        if let Some(v1) = &conn_resp_frame.v1 {
            if let Some(resp) = &v1.connection_response {
                use sharing_proto::connection_response_frame::Status;
                match resp.status {
                    Some(s) if s == Status::Accept as i32 => {
                        info!("Transfer accepted by peer");
                    }
                    Some(s) => {
                        return Err(anyhow!("Transfer rejected by peer (status={})", s));
                    }
                    None => {
                        return Err(anyhow!("Missing status in ConnectionResponse"));
                    }
                }
            } else {
                return Err(anyhow!("Missing connection_response in response frame"));
            }
        } else {
            return Err(anyhow!("Missing v1 in ConnectionResponse frame"));
        }

        // 4. Send file as OfflineFrame PayloadTransfer (via SecureMessage)
        let transfer = OfflineFrame::new_payload_transfer(payload_id, file_name.clone(), file_size);
        self.connection.send_secure(&transfer).await?;

        // 5. Stream file chunks
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

            let chunk = OfflineFrame::new_payload_chunk(offset, buffer[..bytes_read].to_vec());
            self.connection.send_secure(&chunk).await?;

            offset += bytes_read as i64;

            if let Some(cb) = PROGRESS_CB.get() {
                cb(offset, file_size, &file_name);
            }

            info!("Sent {}/{} bytes for {}", offset, file_size, file_name);
        }

        info!("File sent: {}", file_name);
        Ok(())
    }

    pub async fn send_files(&mut self, file_paths: &[&Path]) -> Result<()> {
        for path in file_paths {
            self.send_file(path).await?;
        }
        Ok(())
    }
}
