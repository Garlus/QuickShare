#[cfg(test)]
mod tests {
    use crate::protocol::*;
    use crate::connections_proto::v1_frame::FrameType;

    #[test]
    fn test_roundtrip_connection_request() {
        let frame = OfflineFrame::new_connection_request(
            "test-uuid".to_string(),
            b"TestDevice".to_vec(),
            b"endpoint_info".to_vec(),
            Medium::WifiLan,
            3,
        );
        let wire = WireFrame::encode(&frame).unwrap();
        let parsed = OfflineFrame::decode(&wire[..]).unwrap();
        let v1 = parsed.v1.as_ref().unwrap();
        assert_eq!(v1.r#type, Some(FrameType::ConnectionRequest as i32));
        let req = v1.connection_request.as_ref().unwrap();
        assert_eq!(req.endpoint_id.as_deref(), Some("test-uuid"));
    }

    #[test]
    fn test_roundtrip_connection_response() {
        let frame = OfflineFrame::new_connection_response(true);
        let wire = WireFrame::encode(&frame).unwrap();
        let parsed = OfflineFrame::decode(&wire[..]).unwrap();
        let v1 = parsed.v1.as_ref().unwrap();
        let resp = v1.connection_response.as_ref().unwrap();
        assert_eq!(resp.response, Some(ResponseStatus::Accept as i32));
    }

    #[test]
    fn test_ukey2_message_types() {
        let client_init = Ukey2ClientInit {
            version: Some(1),
            random: Some(vec![0u8; 32]),
            cipher_commitments: Vec::new(),
            next_protocol: Some("AES_256_CBC-HMAC_SHA256".to_string()),
        };
        let msg = Ukey2Message::client_init(&client_init);
        assert_eq!(msg.message_type, Some(ukey2_message::Type::ClientInit as i32));
    }
}
