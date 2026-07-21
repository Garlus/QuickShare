use crate::protocol::*;
use crate::transfer::encryption::{
    CryptoContext, SecureMessageKeys, generate_ecdh_keypair, ecdh_shared_secret,
    finalize_key_exchange,
};
use crate::discovery::utils::DeviceType;
use anyhow::{Result, anyhow};
use rand::RngCore;
use sha2::{Sha512, Digest};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use std::net::SocketAddr;
use tracing::info;

pub const DEFAULT_PORT: u16 = 5721;

const UKEY2_VERSION: i32 = 1;
const NEXT_PROTOCOL: &str = "AES_256_CBC-HMAC_SHA256";

/// Connection state after UKEY2 handshake.
pub struct Connection {
    stream: TcpStream,
    addr: SocketAddr,
    endpoint_id: String,
    crypto: CryptoContext,
    is_initiator: bool,
}

impl Connection {
    /// Connect to a remote device (initiator/outbound).
    pub async fn connect(
        addr: SocketAddr,
        endpoint_id: String,
        device_name: &str,
        device_type: i32,
        crypto: CryptoContext,
    ) -> Result<Self> {
        info!("Connecting to {}...", addr);
        let stream = TcpStream::connect(addr).await
            .map_err(|e| anyhow!("Failed to connect to {}: {}", addr, e))?;

        let mut conn = Connection {
            stream,
            addr,
            endpoint_id: endpoint_id.clone(),
            crypto,
            is_initiator: true,
        };

        conn.perform_handshake_initiator(&endpoint_id, device_name, device_type).await?;
        Ok(conn)
    }

    /// Accept an incoming connection (responder/inbound).
    pub async fn accept(
        stream: TcpStream,
        device_name: &str,
        device_type: i32,
        crypto: CryptoContext,
    ) -> Result<Self> {
        let addr = stream.peer_addr()?;
        info!("Accepting connection from {}...", addr);

        let mut conn = Connection {
            stream,
            addr,
            endpoint_id: String::new(),
            crypto,
            is_initiator: false,
        };

        conn.perform_handshake_responder(device_name, device_type).await?;
        Ok(conn)
    }

    // === UKEY2 Handshake: Initiator (outbound) ===

    async fn perform_handshake_initiator(
        &mut self,
        endpoint_id: &str,
        device_name: &str,
        device_type: i32,
    ) -> Result<()> {
        // Step 1: Send OfflineFrame{ConnectionRequest}
        let endpoint_name = hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "Unknown".to_string());
        let dt = match device_type {
            1 => DeviceType::Phone,
            2 => DeviceType::Tablet,
            3 => DeviceType::Laptop,
            _ => DeviceType::Laptop,
        };
        let endpoint_info = crate::discovery::utils::gen_mdns_endpoint_info(dt, device_name)
            .into_bytes();

        let conn_req = OfflineFrame::new_connection_request(
            endpoint_id.to_string(),
            endpoint_name.into_bytes(),
            endpoint_info,
            Medium::WifiLan,
            device_type,
        );
        WireFrame::send_msg(&mut self.stream, &conn_req).await?;
        info!("Sent ConnectionRequest");

        // Step 2: Generate UKEY2 ClientInit
        let (ecdh_secret, ecdh_public_bytes) = generate_ecdh_keypair();

        // Generate random 32 bytes
        let mut random = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut random);

        // Build a tentative ClientFinish to get its SHA-512 commitment
        let tentative_finish = Ukey2ClientFinished {
            public_key: Some(ecdh_public_bytes.clone()),
        };
        let finish_data = prost::Message::encode_to_vec(&tentative_finish);
        let commitment = Sha512::digest(&finish_data);

        let client_init = Ukey2ClientInit {
            version: Some(UKEY2_VERSION),
            random: Some(random.to_vec()),
            cipher_commitments: vec![
                ukey2_client_init::CipherCommitment {
                    handshake_cipher: Some(Ukey2HandshakeCipher::P256Sha512 as i32),
                    commitment: Some(commitment.to_vec()),
                },
            ],
            next_protocol: Some(NEXT_PROTOCOL.to_string()),
        };

        // Store client_init serialized data for key derivation later
        let client_init_data = prost::Message::encode_to_vec(&client_init);
        let client_init_msg = Ukey2Message::client_init(&client_init);
        WireFrame::send_msg(&mut self.stream, &client_init_msg).await?;
        info!("Sent UKEY2 ClientInit");

        // Step 3: Receive UKEY2 ServerInit
        let server_init_msg: Ukey2Message = WireFrame::recv_msg(&mut self.stream).await?;
        let server_init_data = server_init_msg.message_data
            .ok_or_else(|| anyhow!("ServerInit missing message_data"))?;
        let server_init: securegcm_proto::Ukey2ServerInit =
            prost::Message::decode(&server_init_data[..])
                .map_err(|e| anyhow!("Failed to decode ServerInit: {}", e))?;

        let peer_public_key = server_init.public_key
            .ok_or_else(|| anyhow!("ServerInit missing public_key"))?;
        info!("Received UKEY2 ServerInit");

        // Step 4: ECDH key exchange
        let shared_secret = ecdh_shared_secret(&ecdh_secret, &peer_public_key)?;

        // Step 5: Finalize key exchange
        let kx = finalize_key_exchange(
            &shared_secret,
            true, // we are initiator
            &client_init_data,
            &server_init_data,
        )?;

        // Step 6: Send Ukey2ClientFinished
        let client_finish = Ukey2ClientFinished {
            public_key: Some(ecdh_public_bytes),
        };
        let finish_msg = Ukey2Message::client_finish(&client_finish);
        WireFrame::send_msg(&mut self.stream, &finish_msg).await?;
        info!("Sent UKEY2 ClientFinish");

        // Step 7: Initialize crypto keys
        let keys = SecureMessageKeys::from_key_exchange(&kx, true);
        self.crypto.init(keys).await;

        // Step 8: Receive connection response (plaintext OfflineFrame)
        let conn_resp: OfflineFrame = WireFrame::recv_msg(&mut self.stream).await?;
        let v1 = conn_resp.v1.as_ref()
            .ok_or_else(|| anyhow!("ConnectionResponse missing v1"))?;
        let resp = v1.connection_response.as_ref()
            .ok_or_else(|| anyhow!("Missing connection_response"))?;

        match resp.response {
            Some(s) if s == ResponseStatus::Accept as i32 => {
                info!("Connection accepted by {}", self.addr);
                Ok(())
            }
            _ => Err(anyhow!("Connection rejected by {}", self.addr)),
        }
    }

    // === UKEY2 Handshake: Responder (inbound) ===

    async fn perform_handshake_responder(
        &mut self,
        _device_name: &str,
        _device_type: i32,
    ) -> Result<()> {
        // Step 1: Receive OfflineFrame{ConnectionRequest}
        let conn_req: OfflineFrame = WireFrame::recv_msg(&mut self.stream).await?;
        let v1 = conn_req.v1.as_ref()
            .ok_or_else(|| anyhow!("ConnectionRequest missing v1"))?;
        let req = v1.connection_request.as_ref()
            .ok_or_else(|| anyhow!("Missing connection_request"))?;

        self.endpoint_id = req.endpoint_id.clone()
            .ok_or_else(|| anyhow!("Missing endpoint_id"))?;
        info!("Received ConnectionRequest from endpoint: {}", self.endpoint_id);

        // Step 2: Receive UKEY2 ClientInit
        let client_init_msg: Ukey2Message = WireFrame::recv_msg(&mut self.stream).await?;
        let client_init_data = client_init_msg.message_data
            .ok_or_else(|| anyhow!("ClientInit missing message_data"))?;
        let client_init: Ukey2ClientInit =
            prost::Message::decode(&client_init_data[..])
                .map_err(|e| anyhow!("Failed to decode ClientInit: {}", e))?;
        info!("Received UKEY2 ClientInit");

        // Step 3: Generate UKEY2 ServerInit
        let (ecdh_secret, ecdh_public_bytes) = generate_ecdh_keypair();

        let mut random = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut random);

        let server_init = securegcm_proto::Ukey2ServerInit {
            version: Some(UKEY2_VERSION),
            random: Some(random.to_vec()),
            handshake_cipher: Some(Ukey2HandshakeCipher::P256Sha512 as i32),
            public_key: Some(ecdh_public_bytes.clone()),
        };

        let server_init_data = prost::Message::encode_to_vec(&server_init);
        let server_init_msg = Ukey2Message::server_init(&server_init);
        WireFrame::send_msg(&mut self.stream, &server_init_msg).await?;
        info!("Sent UKEY2 ServerInit");

        // Step 4: Receive UKEY2 ClientFinish
        let client_finish_msg: Ukey2Message = WireFrame::recv_msg(&mut self.stream).await?;
        let finish_data = client_finish_msg.message_data
            .ok_or_else(|| anyhow!("ClientFinish missing message_data"))?;
        let client_finish: Ukey2ClientFinished =
            prost::Message::decode(&finish_data[..])
                .map_err(|e| anyhow!("Failed to decode ClientFinish: {}", e))?;

        let peer_public_key = client_finish.public_key
            .ok_or_else(|| anyhow!("ClientFinish missing public_key"))?;
        info!("Received UKEY2 ClientFinish");

        // Step 5: Verify the ClientFinish commitment
        let received_commitment = Sha512::digest(&finish_data);
        if let Some(expected_commitment) = client_init.cipher_commitments.first()
            .and_then(|c| c.commitment.as_ref())
        {
            if received_commitment.as_slice() != expected_commitment.as_slice() {
                return Err(anyhow!("ClientFinish commitment mismatch"));
            }
        }

        // Step 6: ECDH key exchange
        let shared_secret = ecdh_shared_secret(&ecdh_secret, &peer_public_key)?;

        // Step 7: Finalize key exchange (server side: we are NOT initiator)
        let kx = finalize_key_exchange(
            &shared_secret,
            false, // we are responder
            &server_init_data,
            &client_init_data,
        )?;

        // Step 8: Initialize crypto keys
        let keys = SecureMessageKeys::from_key_exchange(&kx, false);
        self.crypto.init(keys).await;

        // Step 9: Send ConnectionResponse (plaintext)
        let conn_resp = OfflineFrame::new_connection_response(true);
        WireFrame::send_msg(&mut self.stream, &conn_resp).await?;
        info!("Sent ConnectionResponse (Accept) to {}", self.addr);

        Ok(())
    }

    /// Send any protobuf message wrapped in SecureMessage.
    pub async fn send_secure(&mut self, msg: &impl prost::Message) -> Result<()> {
        let msg_bytes = prost::Message::encode_to_vec(msg);
        let encrypted = self.crypto.encrypt(&msg_bytes).await?;
        let len = encrypted.len() as u32;
        self.stream.write_all(&len.to_be_bytes()).await
            .map_err(|e| anyhow!("Failed to send secure length: {}", e))?;
        self.stream.write_all(&encrypted).await
            .map_err(|e| anyhow!("Failed to send secure data: {}", e))?;
        Ok(())
    }

    /// Receive a SecureMessage and decode the inner protobuf.
    pub async fn recv_secure<T: prost::Message + Default>(&mut self) -> Result<T> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await
            .map_err(|e| anyhow!("Failed to read secure length: {}", e))?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut encrypted = vec![0u8; len];
        self.stream.read_exact(&mut encrypted).await
            .map_err(|e| anyhow!("Failed to read secure payload: {}", e))?;

        let (msg_bytes, _seq) = self.crypto.decrypt(&encrypted).await?;
        T::decode(&msg_bytes[..])
            .map_err(|e| anyhow!("Failed to decode secure message: {}", e))
    }

    /// Send raw bytes with a 4-byte length prefix (no encryption).
    pub async fn send_raw(&mut self, data: &[u8]) -> Result<()> {
        let len = data.len() as u32;
        self.stream.write_all(&len.to_be_bytes()).await
            .map_err(|e| anyhow!("Failed to send raw length: {}", e))?;
        self.stream.write_all(data).await
            .map_err(|e| anyhow!("Failed to send raw data: {}", e))?;
        Ok(())
    }

    /// Receive raw bytes with a 4-byte length prefix (no encryption).
    pub async fn recv_raw(&mut self) -> Result<Vec<u8>> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await
            .map_err(|e| anyhow!("Failed to read raw length: {}", e))?;
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data).await
            .map_err(|e| anyhow!("Failed to read raw data: {}", e))?;
        Ok(data)
    }

    /// Disconnect gracefully.
    pub async fn disconnect(&mut self) -> Result<()> {
        if self.crypto.is_initialized().await {
            let disc = OfflineFrame::new_disconnection();
            self.send_secure(&disc).await.ok();
        }
        self.stream.shutdown().await?;
        info!("Disconnected from {}", self.addr);
        Ok(())
    }

    pub fn endpoint_id(&self) -> &str {
        &self.endpoint_id
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn is_initiator(&self) -> bool {
        self.is_initiator
    }
}

/// Listen for incoming QuickShare connections.
pub struct ConnectionListener {
    listener: TcpListener,
    port: u16,
}

impl ConnectionListener {
    pub async fn new(port: u16) -> Result<Self> {
        let addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&addr).await
            .map_err(|e| anyhow!("Failed to bind to {}: {}", addr, e))?;
        info!("Listening for QuickShare connections on port {}", port);
        Ok(ConnectionListener { listener, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Accept a single incoming connection (performs UKEY2 handshake).
    pub async fn accept(
        &self,
        device_name: &str,
        device_type: i32,
        crypto: CryptoContext,
    ) -> Result<Connection> {
        let (stream, addr) = self.listener.accept().await
            .map_err(|e| anyhow!("Failed to accept connection: {}", e))?;
        info!("Incoming connection from {}", addr);
        let conn = Connection::accept(stream, device_name, device_type, crypto).await?;
        Ok(conn)
    }

    pub fn stop(self) {
        info!("Stopped listening on port {}", self.port);
    }
}
