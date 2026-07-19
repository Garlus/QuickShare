use super::Frame;
#[cfg(test)]
use super::V1Frame;
use anyhow::{Result, anyhow};

/// A raw frame that includes the 4-byte length prefix used by Nearby Connections.
pub struct WireFrame;

impl WireFrame {
    /// Encode a frame with a 4-byte big-endian length prefix.
    pub fn encode(frame: &Frame) -> Result<Vec<u8>> {
        let payload = frame.to_bytes()?;
        let len = payload.len() as u32;
        let mut wire = Vec::with_capacity(4 + payload.len());
        wire.extend_from_slice(&len.to_be_bytes());
        wire.extend_from_slice(&payload);
        Ok(wire)
    }

    /// Parse raw bytes into a protobuf frame (expects 4-byte big-endian length prefix).
    /// Returns the decoded frame and the number of bytes consumed.
    pub fn decode(data: &[u8]) -> Result<(Frame, usize)> {
        if data.len() < 4 {
            return Err(anyhow!("Frame too short: missing length prefix"));
        }
        let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + len {
            return Err(anyhow!(
                "Frame truncated: expected {} bytes, got {}",
                4 + len,
                data.len()
            ));
        }
        let frame = Frame::from_bytes(&data[4..4 + len])?;
        Ok((frame, 4 + len))
    }

    /// Parse multiple frames from a stream of bytes.
    pub fn decode_stream(data: &[u8]) -> Result<Vec<(Frame, usize)>> {
        let mut frames = Vec::new();
        let mut offset = 0;
        while offset < data.len() {
            let (frame, consumed) = Self::decode(&data[offset..])?;
            frames.push((frame, consumed));
            offset += consumed;
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::{Frame, V1Frame, WireFrame};
    use crate::protocol::{Medium, DeviceType};

    #[test]
    fn test_roundtrip_device_profile() {
        let v1 = V1Frame::new_device_profile("TestDevice", DeviceType::Desktop);
        let frame = Frame::new_v1(v1);
        let wire = WireFrame::encode(&frame).unwrap();
        let (parsed, _) = WireFrame::decode(&wire).unwrap();
        let parsed_v1 = parsed.v1.as_ref().unwrap();
        let profile = parsed_v1.device_profile.as_ref().unwrap();
        assert_eq!(profile.device_name.as_deref(), Some("TestDevice"));
    }

    #[test]
    fn test_roundtrip_connection_request() {
        let endpoint_id = b"endpoint123".to_vec();
        let cert = b"certdata".to_vec();
        let v1 = V1Frame::new_connection_request(endpoint_id.clone(), cert.clone(), Medium::WifiLan);
        let frame = Frame::new_v1(v1);
        let wire = WireFrame::encode(&frame).unwrap();
        let (parsed, _) = WireFrame::decode(&wire).unwrap();
        let parsed_v1 = parsed.v1.as_ref().unwrap();
        let req = parsed_v1.connection_request.as_ref().unwrap();
        assert_eq!(req.endpoint_id.as_deref(), Some(&endpoint_id[..]));
        assert_eq!(req.certificate.as_deref(), Some(&cert[..]));
    }

    #[test]
    fn test_multiple_frames_in_stream() {
        let frame1 = Frame::new_v1(V1Frame::new_keep_alive(false));
        let frame2 = Frame::new_v1(V1Frame::new_disconnection());
        let wire1 = WireFrame::encode(&frame1).unwrap();
        let wire2 = WireFrame::encode(&frame2).unwrap();
        let mut stream = Vec::new();
        stream.extend_from_slice(&wire1);
        stream.extend_from_slice(&wire2);
        let parsed = WireFrame::decode_stream(&stream).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn test_frame_too_short() {
        let result = WireFrame::decode(&[0x00, 0x00]);
        assert!(result.is_err());
    }

    #[test]
    fn test_frame_truncated() {
        let v1 = V1Frame::new_device_profile("Test", DeviceType::Phone);
        let frame = Frame::new_v1(v1);
        let wire = WireFrame::encode(&frame).unwrap();
        let truncated = &wire[..wire.len() - 2];
        let result = WireFrame::decode(truncated);
        assert!(result.is_err());
    }
}
