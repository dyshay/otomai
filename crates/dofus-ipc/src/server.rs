//! IPC server (runs on Auth side). Accepts connections from World servers.

use crate::IpcEnvelope;
use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

/// Start the IPC server. Returns a receiver for incoming messages.
pub async fn start(
    addr: &str,
) -> Result<(mpsc::UnboundedReceiver<(IpcEnvelope, mpsc::UnboundedSender<IpcEnvelope>)>, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("IPC server listening on {}", addr);

    let (tx, rx) = mpsc::unbounded_channel();

    let handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    tracing::info!("IPC client connected: {}", peer);
                    let tx = tx.clone();
                    tokio::spawn(handle_ipc_client(stream, tx));
                }
                Err(e) => {
                    tracing::warn!("IPC accept error: {}", e);
                }
            }
        }
    });

    Ok((rx, handle))
}

async fn handle_ipc_client(
    stream: TcpStream,
    incoming: mpsc::UnboundedSender<(IpcEnvelope, mpsc::UnboundedSender<IpcEnvelope>)>,
) {
    let (reply_tx, mut reply_rx) = mpsc::unbounded_channel::<IpcEnvelope>();
    let (reader, writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);
    let mut writer = writer;

    let write_task = tokio::spawn(async move {
        while let Some(msg) = reply_rx.recv().await {
            if let Ok(data) = serde_json::to_vec(&msg) {
                let len = data.len() as u32;
                let _ = writer.write_all(&len.to_be_bytes()).await;
                let _ = writer.write_all(&data).await;
            }
        }
    });

    loop {
        let mut len_buf = [0u8; 4];
        if reader.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 1_000_000 { break; }

        let mut buf = vec![0u8; len];
        if reader.read_exact(&mut buf).await.is_err() { break; }

        match serde_json::from_slice::<IpcEnvelope>(&buf) {
            Ok(envelope) => { let _ = incoming.send((envelope, reply_tx.clone())); }
            Err(e) => { tracing::warn!("IPC parse error: {}", e); }
        }
    }

    write_task.abort();
    tracing::info!("IPC client disconnected");
}

/// Send an IPC message to a connected client.
pub fn send(tx: &mpsc::UnboundedSender<IpcEnvelope>, msg_type: &str, payload: &impl serde::Serialize) {
    let envelope = IpcEnvelope {
        msg_type: msg_type.to_string(),
        payload: serde_json::to_value(payload).unwrap_or_default(),
    };
    let _ = tx.send(envelope);
}
