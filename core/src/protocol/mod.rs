use anyhow::Result;
use anyhow::anyhow;

// Generated proto modules
pub mod securemessage {
    include!(concat!(env!("OUT_DIR"), "/securemessage.rs"));
}

pub mod securegcm_proto {
    include!(concat!(env!("OUT_DIR"), "/securegcm.rs"));
}

pub mod connections_proto {
    include!(concat!(env!("OUT_DIR"), "/location.nearby.connections.rs"));
}

pub mod sharing_proto {
    include!(concat!(env!("OUT_DIR"), "/sharing.nearby.rs"));
}

// Re-export commonly used types
pub use connections_proto::{
    OfflineFrame,
    V1Frame as OfflineV1Frame,
    ConnectionRequestFrame,
    ConnectionResponseFrame,
    PayloadTransferFrame,
    KeepAliveFrame,
    DisconnectionFrame,
    PairedKeyEncryptionFrame as OfflinePairedKeyEncryption,
};

pub use securegcm_proto::{
    Ukey2Message,
    Ukey2ClientInit,
    Ukey2ClientFinished,
    Ukey2ServerInit,
    ukey2_message,
    ukey2_client_init,
    Ukey2HandshakeCipher,
};

pub use sharing_proto::{
    Frame as SharingFrame,
    V1Frame as SharingV1Frame,
    IntroductionFrame,
    PairedKeyEncryptionFrame as SharingPairedKeyEncryption,
    PairedKeyResultFrame as SharingPairedKeyResult,
    FileMetadata,
};

pub use securemessage::{
    SecureMessage,
    Header as SecureHeader,
    HeaderAndBody,
};

pub use securegcm_proto::DeviceToDeviceMessage;

// === Helper type aliases ===

pub type Medium = connections_proto::connection_request_frame::Medium;
pub type OfflineFrameType = connections_proto::v1_frame::FrameType;
pub type ResponseStatus = connections_proto::connection_response_frame::ResponseStatus;
pub type SharingFrameType = sharing_proto::v1_frame::FrameType;
pub type PairedKeyResultStatus = sharing_proto::paired_key_result_frame::Status;

// === Wire frame helpers ===

pub struct WireFrame;

impl WireFrame {
    pub fn encode<T: prost::Message>(msg: &T) -> Result<Vec<u8>> {
        let payload = prost::Message::encode_to_vec(msg);
        let len = payload.len() as u32;
        let mut wire = Vec::with_capacity(4 + payload.len());
        wire.extend_from_slice(&len.to_be_bytes());
        wire.extend_from_slice(&payload);
        Ok(wire)
    }

    pub async fn recv_msg<T: prost::Message + Default>(
        reader: &mut (impl tokio::io::AsyncReadExt + Unpin),
    ) -> Result<T> {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await
            .map_err(|e| anyhow!("Read failed: {}", e))?;
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload).await
            .map_err(|e| anyhow!("Read failed: {}", e))?;
        T::decode(&payload[..])
            .map_err(|e| anyhow!("Failed to decode protobuf: {}", e))
    }

    pub async fn send_msg<T: prost::Message>(
        writer: &mut (impl tokio::io::AsyncWriteExt + Unpin),
        msg: &T,
    ) -> Result<()> {
        let wire = Self::encode(msg)?;
        writer.write_all(&wire).await
            .map_err(|e| anyhow!("Write failed: {}", e))?;
        Ok(())
    }
}

// === Convenience constructors ===

impl OfflineFrame {
    pub fn new_connection_request(
        endpoint_id: String,
        endpoint_name: Vec<u8>,
        endpoint_info: Vec<u8>,
        medium: Medium,
        device_type: i32,
    ) -> Self {
        let mut mediums = Vec::new();
        mediums.push(medium as i32);

        OfflineFrame {
            version: Some(connections_proto::offline_frame::Version::V1 as i32),
            v1: Some(OfflineV1Frame {
                r#type: Some(OfflineFrameType::ConnectionRequest as i32),
                connection_request: Some(ConnectionRequestFrame {
                    endpoint_id: Some(endpoint_id),
                    endpoint_name: Some(endpoint_name),
                    handshake_data: None,
                    nonce: None,
                    mediums,
                    endpoint_info: Some(endpoint_info),
                    medium_metadata: None,
                    keep_alive_interval_millis: None,
                    keep_alive_timeout_millis: None,
                    device_type: Some(device_type),
                    device_info: None,
                }),
                connection_response: None,
                payload_transfer: None,
                keep_alive: None,
                disconnection: None,
                paired_key_encryption: None,
            }),
        }
    }

    pub fn new_connection_response(accept: bool) -> Self {
        let status = if accept {
            ResponseStatus::Accept as i32
        } else {
            ResponseStatus::Reject as i32
        };

        OfflineFrame {
            version: Some(connections_proto::offline_frame::Version::V1 as i32),
            v1: Some(OfflineV1Frame {
                r#type: Some(OfflineFrameType::ConnectionResponse as i32),
                connection_request: None,
                connection_response: Some(ConnectionResponseFrame {
                    #[allow(deprecated)]
                    status: None,
                    handshake_data: None,
                    response: Some(status),
                    os_info: None,
                    multiplex_socket_bitmask: None,
                    nearby_connections_version: None,
                }),
                payload_transfer: None,
                keep_alive: None,
                disconnection: None,
                paired_key_encryption: None,
            }),
        }
    }

    pub fn new_keep_alive(ack: bool) -> Self {
        OfflineFrame {
            version: Some(connections_proto::offline_frame::Version::V1 as i32),
            v1: Some(OfflineV1Frame {
                r#type: Some(OfflineFrameType::KeepAlive as i32),
                connection_request: None,
                connection_response: None,
                payload_transfer: None,
                keep_alive: Some(KeepAliveFrame { ack: Some(ack) }),
                disconnection: None,
                paired_key_encryption: None,
            }),
        }
    }

    pub fn new_disconnection() -> Self {
        OfflineFrame {
            version: Some(connections_proto::offline_frame::Version::V1 as i32),
            v1: Some(OfflineV1Frame {
                r#type: Some(OfflineFrameType::Disconnection as i32),
                connection_request: None,
                connection_response: None,
                payload_transfer: None,
                keep_alive: None,
                disconnection: Some(DisconnectionFrame {
                    request_safe_to_disconnect: None,
                    ack_safe_to_disconnect: None,
                }),
                paired_key_encryption: None,
            }),
        }
    }

    pub fn new_payload_transfer(
        payload_id: i64,
        file_name: String,
        file_size: i64,
    ) -> Self {
        use connections_proto::payload_transfer_frame::PayloadHeader;
        use connections_proto::payload_transfer_frame::payload_header;
        use connections_proto::payload_transfer_frame::PacketType;

        OfflineFrame {
            version: Some(connections_proto::offline_frame::Version::V1 as i32),
            v1: Some(OfflineV1Frame {
                r#type: Some(OfflineFrameType::PayloadTransfer as i32),
                connection_request: None,
                connection_response: None,
                payload_transfer: Some(PayloadTransferFrame {
                    packet_type: Some(PacketType::Control as i32),
                    payload_header: Some(PayloadHeader {
                        id: Some(payload_id),
                        r#type: Some(payload_header::PayloadType::File as i32),
                        total_size: Some(file_size),
                        is_sensitive: None,
                        file_name: Some(file_name),
                        parent_folder: None,
                    }),
                    payload_chunk: None,
                    control_message: None,
                }),
                keep_alive: None,
                disconnection: None,
                paired_key_encryption: None,
            }),
        }
    }

    pub fn new_payload_chunk(offset: i64, body: Vec<u8>) -> Self {
        use connections_proto::payload_transfer_frame::PayloadChunk;
        use connections_proto::payload_transfer_frame::PacketType;

        OfflineFrame {
            version: Some(connections_proto::offline_frame::Version::V1 as i32),
            v1: Some(OfflineV1Frame {
                r#type: Some(OfflineFrameType::PayloadTransfer as i32),
                connection_request: None,
                connection_response: None,
                payload_transfer: Some(PayloadTransferFrame {
                    packet_type: Some(PacketType::Data as i32),
                    payload_header: None,
                    payload_chunk: Some(PayloadChunk {
                        flags: None,
                        offset: Some(offset),
                        body: Some(body),
                    }),
                    control_message: None,
                }),
                keep_alive: None,
                disconnection: None,
                paired_key_encryption: None,
            }),
        }
    }
}

impl Ukey2Message {
    pub fn client_init(init: &Ukey2ClientInit) -> Self {
        Ukey2Message {
            message_type: Some(ukey2_message::Type::ClientInit as i32),
            message_data: Some(prost::Message::encode_to_vec(init)),
        }
    }

    pub fn server_init(init: &Ukey2ServerInit) -> Self {
        Ukey2Message {
            message_type: Some(ukey2_message::Type::ServerInit as i32),
            message_data: Some(prost::Message::encode_to_vec(init)),
        }
    }

    pub fn client_finish(finish: &Ukey2ClientFinished) -> Self {
        Ukey2Message {
            message_type: Some(ukey2_message::Type::ClientFinish as i32),
            message_data: Some(prost::Message::encode_to_vec(finish)),
        }
    }
}

impl SharingFrame {
    pub fn new_introduction(files: Vec<FileMetadata>) -> Self {
        SharingFrame {
            version: Some(sharing_proto::frame::Version::V1 as i32),
            v1: Some(SharingV1Frame {
                r#type: Some(SharingFrameType::Introduction as i32),
                introduction: Some(IntroductionFrame {
                    file_metadata: files,
                    text_metadata: Vec::new(),
                    required_package: None,
                }),
                connection_response: None,
                paired_key_encryption: None,
                paired_key_result: None,
            }),
        }
    }

    pub fn new_paired_key_result(status: PairedKeyResultStatus) -> Self {
        SharingFrame {
            version: Some(sharing_proto::frame::Version::V1 as i32),
            v1: Some(SharingV1Frame {
                r#type: Some(SharingFrameType::PairedKeyResult as i32),
                introduction: None,
                connection_response: None,
                paired_key_encryption: None,
                paired_key_result: Some(SharingPairedKeyResult {
                    status: Some(status as i32),
                }),
            }),
        }
    }
}
