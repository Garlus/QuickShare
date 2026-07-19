use crate::protocol::{Frame, V1Frame, WireFrame, FrameType, Medium, ConnectionStatus};
use crate::transfer::encryption::CryptoContext;
use anyhow::{Result, anyhow};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

pub const DEFAULT_PORT: u16 = 5721;

/// Represents a connection to a remote QuickShare device.
pub struct Connection {
    stream: TcpStream,
    addr: SocketAddr,
    endpoint_id: Vec<u8>,
    #[allow(dead_code)]
    crypto: Arc<CryptoContext>,
    authenticated: Arc<Mutex<bool>>,
}

impl Connection {
    /// Connect to a remote device (initiator).
    pub async fn connect(
        addr: SocketAddr,
        endpoint_id: Vec<u8>,
        certificate: Vec<u8>,
        crypto: Arc<CryptoContext>,
    ) -> Result<Self> {
        info!("Connecting to {}...", addr);
        let stream = TcpStream::connect(addr).await
            .map_err(|e| anyhow!("Failed to connect to {}: {}", addr, e))?;

        let mut conn = Connection {
            stream,
            addr,
            endpoint_id,
            crypto,
            authenticated: Arc::new(Mutex::new(false)),
        };

        // Send connection request
        let req = V1Frame::new_connection_request(
            conn.endpoint_id.clone(),
            certificate,
            Medium::WifiLan,
        );
        conn.send_frame(&Frame::new_v1(req)).await?;

        // Wait for connection response
        let (frame, _) = conn.recv_frame().await?;
        let v1 = frame.v1.as_ref()
            .ok_or_else(|| anyhow!("Missing V1 frame"))?;
        match v1.r#type {
            Some(t) if t == FrameType::ConnectionResponse as i32 => {
                let resp = v1.connection_response.as_ref()
                    .ok_or_else(|| anyhow!("Missing connection response"))?;
                match resp.status {
                    Some(s) if s == ConnectionStatus::Accept as i32 => {
                        info!("Connection accepted by {}", addr);
                        *conn.authenticated.lock().await = true;
                    }
                    _ => return Err(anyhow!("Connection rejected by {}", addr)),
                }
            }
            _ => return Err(anyhow!("Unexpected response from {}", addr)),
        }

        Ok(conn)
    }

    /// Accept an incoming connection (responder).
    pub async fn accept(stream: TcpStream, endpoint_id: Vec<u8>, crypto: Arc<CryptoContext>) -> Result<Self> {
        let addr = stream.peer_addr()?;
        info!("Accepting connection from {}...", addr);

        let mut conn = Connection {
            stream,
            addr,
            endpoint_id,
            crypto,
            authenticated: Arc::new(Mutex::new(false)),
        };

        // Wait for connection request
        let (frame, _) = conn.recv_frame().await?;
        let v1 = frame.v1.as_ref()
            .ok_or_else(|| anyhow!("Missing V1 frame"))?;

        match v1.r#type {
            Some(t) if t == FrameType::ConnectionRequest as i32 => {
                let req = v1.connection_request.as_ref()
                    .ok_or_else(|| anyhow!("Missing connection request"))?;
                conn.endpoint_id = req.endpoint_id.clone()
                    .ok_or_else(|| anyhow!("Missing endpoint ID"))?;

                // Send accept response
                let resp = V1Frame::new_connection_response(
                    conn.endpoint_id.clone(),
                    ConnectionStatus::Accept,
                );
                conn.send_frame(&Frame::new_v1(resp)).await?;
                info!("Accepted connection from {}", addr);
                *conn.authenticated.lock().await = true;
            }
            _ => {
                // Reject
                let resp = V1Frame::new_connection_response(
                    vec![],
                    ConnectionStatus::Reject,
                );
                conn.send_frame(&Frame::new_v1(resp)).await?;
                return Err(anyhow!("Unexpected frame type from {}", addr));
            }
        }

        Ok(conn)
    }

    /// Send a framed protobuf message over the TCP connection.
    pub async fn send_frame(&mut self, frame: &Frame) -> Result<()> {
        let wire = WireFrame::encode(frame)?;
        self.stream.write_all(&wire).await
            .map_err(|e| anyhow!("Failed to send frame: {}", e))?;
        Ok(())
    }

    /// Receive a framed protobuf message from the TCP connection.
    pub async fn recv_frame(&mut self) -> Result<(Frame, usize)> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await
            .map_err(|e| anyhow!("Failed to read frame length: {}", e))?;

        let len = u32::from_be_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        self.stream.read_exact(&mut payload).await
            .map_err(|e| anyhow!("Failed to read frame payload: {}", e))?;

        let (frame, consumed) = WireFrame::decode(
            &[&len_buf[..], &payload[..]].concat()
        )?;
        Ok((frame, consumed))
    }

    /// Send raw encrypted payload data.
    pub async fn send_raw(&mut self, data: &[u8]) -> Result<()> {
        let len = data.len() as u64;
        self.stream.write_all(&len.to_be_bytes()).await
            .map_err(|e| anyhow!("Failed to send raw data length: {}", e))?;
        self.stream.write_all(data).await
            .map_err(|e| anyhow!("Failed to send raw data: {}", e))?;
        Ok(())
    }

    /// Receive raw payload data.
    pub async fn recv_raw(&mut self) -> Result<Vec<u8>> {
        let mut len_buf = [0u8; 8];
        self.stream.read_exact(&mut len_buf).await
            .map_err(|e| anyhow!("Failed to read raw data length: {}", e))?;
        let len = u64::from_be_bytes(len_buf) as usize;
        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data).await
            .map_err(|e| anyhow!("Failed to read raw data: {}", e))?;
        Ok(data)
    }

    /// Disconnect gracefully.
    pub async fn disconnect(mut self) -> Result<()> {
        let frame = V1Frame::new_disconnection();
        self.send_frame(&Frame::new_v1(frame)).await.ok();
        self.stream.shutdown().await?;
        info!("Disconnected from {}", self.addr);
        Ok(())
    }

    pub fn endpoint_id(&self) -> &[u8] {
        &self.endpoint_id
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn is_authenticated(&self) -> bool {
        self.authenticated.try_lock()
            .map(|guard| *guard)
            .unwrap_or(false)
    }
}

/// Listen for incoming QuickShare connections on a given port.
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

    /// Accept a single incoming connection.
    pub async fn accept(&self, crypto: Arc<CryptoContext>) -> Result<Connection> {
        let (stream, addr) = self.listener.accept().await
            .map_err(|e| anyhow!("Failed to accept connection: {}", e))?;
        info!("Incoming connection from {}", addr);
        let conn = Connection::accept(stream, vec![], crypto).await?;
        Ok(conn)
    }

    /// Stop listening.
    pub fn stop(self) {
        // Listener will be dropped, closing the socket
        info!("Stopped listening on port {}", self.port);
    }
}
