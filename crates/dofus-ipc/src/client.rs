//! IPC client (runs on World side). Connects to Auth's IPC server.

use crate::IpcEnvelope;
use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

pub struct IpcClient {
    write_tx: mpsc::UnboundedSender<IpcEnvelope>,
    read_rx: mpsc::UnboundedReceiver<IpcEnvelope>,
}

impl IpcClient {
    /// Connect to the IPC server.
    pub async fn connect(addr: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        tracing::info!("IPC connected to {}", addr);

        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<IpcEnvelope>();
        let (read_tx, read_rx) = mpsc::unbounded_channel::<IpcEnvelope>();

        let (mut reader, mut writer) = stream.into_split();

        // Write task
        tokio::spawn(async move {
            while let Some(msg) = write_rx.recv().await {
                if let Ok(data) = serde_json::to_vec(&msg) {
                    let len = data.len() as u32;
                    if writer.write_all(&len.to_be_bytes()).await.is_err() { break; }
                    if writer.write_all(&data).await.is_err() { break; }
                }
            }
        });

        // Read task
        tokio::spawn(async move {
            loop {
                let mut len_buf = [0u8; 4];
                if reader.read_exact(&mut len_buf).await.is_err() { break; }
                let len = u32::from_be_bytes(len_buf) as usize;
                if len > 1_000_000 { break; }

                let mut buf = vec![0u8; len];
                if reader.read_exact(&mut buf).await.is_err() { break; }

                match serde_json::from_slice::<IpcEnvelope>(&buf) {
                    Ok(envelope) => { let _ = read_tx.send(envelope); }
                    Err(e) => { tracing::warn!("IPC parse error: {}", e); }
                }
            }
            tracing::warn!("IPC connection lost");
        });

        Ok(Self { write_tx, read_rx })
    }

    /// Send a message to the auth server.
    pub fn send(&self, msg_type: &str, payload: &impl serde::Serialize) {
        let envelope = IpcEnvelope {
            msg_type: msg_type.to_string(),
            payload: serde_json::to_value(payload).unwrap_or_default(),
        };
        let _ = self.write_tx.send(envelope);
    }

    /// Try to receive a message (non-blocking).
    pub fn try_recv(&mut self) -> Option<IpcEnvelope> {
        self.read_rx.try_recv().ok()
    }

    /// Receive next message (blocking).
    pub async fn recv(&mut self) -> Option<IpcEnvelope> {
        self.read_rx.recv().await
    }
}
