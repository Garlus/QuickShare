use crate::protocol::*;
use crate::transfer::connection::Connection;
use crate::transfer::encryption::CryptoContext;
use anyhow::{Result, anyhow};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tracing::info;

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

        // 1. Send Introduction (sharing layer, via SecureMessage)
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

        // 2. Send PairedKeyEncryption (empty — we don't have Google certs)
        let paired_enc = SharingFrame {
            version: Some(sharing_proto::frame::Version::V1 as i32),
            v1: Some(sharing_proto::V1Frame {
                r#type: Some(SharingFrameType::PairedKeyEncryption as i32),
                introduction: None,
                connection_response: None,
                paired_key_encryption: Some(SharingPairedKeyEncryption {
                    signed_data: Some(Vec::new()),
                    secret_id_hash: None,
                    optional_signed_data: None,
                }),
                paired_key_result: None,
            }),
        };
        self.connection.send_secure(&paired_enc).await?;
        info!("Sent PairedKeyEncryption frame");

        // 3. Receive PairedKeyResult (expect UNABLE since no Google certs)
        let paired_result: SharingFrame = self.connection.recv_secure().await?;
        if let Some(v1) = &paired_result.v1 {
            if let Some(result) = &v1.paired_key_result {
                info!("PairedKeyResult status: {:?}", result.status);
            }
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
