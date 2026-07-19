use anyhow::{Result, anyhow};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/quickshare.rs"));
}

pub use proto::{
    Frame, V1Frame, ConnectionRequestFrame,
    ConnectionResponseFrame, PayloadTransferFrame,
    PayloadChunkFrame, KeepAliveFrame, DisconnectionFrame,
    ConnectionResultFrame,
};

pub use proto::v1_frame::FrameType;
pub use proto::connection_request_frame::Medium;
pub use proto::connection_response_frame::Status as ConnectionStatus;
pub use proto::connection_result_frame::Status as ConnectionResult;
pub use proto::device_profile_frame::DeviceType;

pub mod frame;
pub use frame::WireFrame;

impl Frame {
    pub fn new_v1(v1: V1Frame) -> Self {
        Frame {
            version: Some(1), // V1_0 = 1
            v1: Some(v1),
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        prost::Message::encode(self, &mut buf)
            .map_err(|e| anyhow!("Failed to encode frame: {}", e))?;
        Ok(buf)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        prost::Message::decode(data)
            .map_err(|e| anyhow!("Failed to decode frame: {}", e))
    }
}

impl V1Frame {
    pub fn new_device_profile(device_name: &str, device_type: DeviceType) -> Self {
        let profile = proto::DeviceProfileFrame {
            endpoint_id: None,
            device_name: Some(device_name.to_string()),
            device_type: Some(device_type as i32),
        };
        V1Frame {
            r#type: Some(FrameType::DeviceProfile as i32),
            device_profile: Some(profile),
            ..Default::default()
        }
    }

    pub fn new_connection_request(endpoint_id: Vec<u8>, certificate: Vec<u8>, medium: Medium) -> Self {
        let req = ConnectionRequestFrame {
            endpoint_id: Some(endpoint_id),
            certificate: Some(certificate),
            medium: Some(medium as i32),
        };
        V1Frame {
            r#type: Some(FrameType::ConnectionRequest as i32),
            connection_request: Some(req),
            ..Default::default()
        }
    }

    pub fn new_connection_response(endpoint_id: Vec<u8>, status: ConnectionStatus) -> Self {
        let resp = ConnectionResponseFrame {
            status: Some(status as i32),
            endpoint_id: Some(endpoint_id),
        };
        V1Frame {
            r#type: Some(FrameType::ConnectionResponse as i32),
            connection_response: Some(resp),
            ..Default::default()
        }
    }

    pub fn new_payload_transfer(
        payload_id: Vec<u8>,
        file_name: String,
        file_size: i64,
        mime_type: String,
    ) -> Self {
        let file_info = proto::payload_transfer_frame::FileInfo {
            file_name: Some(file_name),
            file_size: Some(file_size),
            mime_type: Some(mime_type),
            payload_chunk_size: None,
            offset: None,
            parent_folder: None,
        };
        let transfer = PayloadTransferFrame {
            payload_id: Some(payload_id),
            r#type: Some(1), // FILE
            file_info: Some(file_info),
            bytes_payload: None,
        };
        V1Frame {
            r#type: Some(FrameType::PayloadTransfer as i32),
            payload_transfer: Some(transfer),
            ..Default::default()
        }
    }

    pub fn new_payload_chunk(payload_id: Vec<u8>, offset: i64, chunk: Vec<u8>) -> Self {
        let chunk_frame = PayloadChunkFrame {
            payload_id: Some(payload_id),
            offset: Some(offset),
            chunk: Some(chunk),
        };
        V1Frame {
            r#type: Some(FrameType::PayloadChunk as i32),
            payload_chunk: Some(chunk_frame),
            ..Default::default()
        }
    }

    pub fn new_keep_alive(response_requested: bool) -> Self {
        V1Frame {
            r#type: Some(FrameType::KeepAlive as i32),
            keep_alive: Some(KeepAliveFrame { response_requested: Some(response_requested) }),
            ..Default::default()
        }
    }

    pub fn new_disconnection() -> Self {
        V1Frame {
            r#type: Some(FrameType::Disconnection as i32),
            disconnection: Some(DisconnectionFrame::default()),
            ..Default::default()
        }
    }

    pub fn new_connection_result(status: ConnectionResult, error_message: Option<String>) -> Self {
        V1Frame {
            r#type: Some(FrameType::ConnectionResult as i32),
            connection_result: Some(ConnectionResultFrame {
                status: Some(status as i32),
                error_message,
            }),
            ..Default::default()
        }
    }
}
