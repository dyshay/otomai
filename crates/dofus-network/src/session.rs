use crate::codec::{DofusCodec, RawMessage, encode_message};
use dofus_io::DofusMessage;
use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tracing;

pub type DofusFramed = Framed<TcpStream, DofusCodec>;

/// A network session wrapping a framed TCP connection.
pub struct Session {
    framed: DofusFramed,
    next_instance_id: u32,
}

impl Session {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            framed: Framed::new(stream, DofusCodec::new()),
            next_instance_id: 0,
        }
    }

    /// Send a typed protocol message.
    pub async fn send<M: DofusMessage>(&mut self, msg: &M) -> anyhow::Result<()> {
        let instance_id = self.next_instance_id;
        self.next_instance_id = self.next_instance_id.wrapping_add(1);
        let raw = encode_message(msg, instance_id);
        tracing::debug!(
            message_id = raw.message_id,
            instance_id = raw.instance_id,
            payload_len = raw.payload.len(),
            "Sending message"
        );
        self.framed.send(raw).await?;
        Ok(())
    }

    /// Receive the next raw message from the client.
    pub async fn recv(&mut self) -> anyhow::Result<Option<RawMessage>> {
        match self.framed.next().await {
            Some(Ok(raw)) => {
                tracing::debug!(
                    message_id = raw.message_id,
                    instance_id = raw.instance_id,
                    payload_len = raw.payload.len(),
                    "Received message"
                );
                Ok(Some(raw))
            }
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Send a raw message directly.
    pub async fn send_raw(&mut self, raw: RawMessage) -> anyhow::Result<()> {
        tracing::debug!(
            message_id = raw.message_id,
            payload_len = raw.payload.len(),
            "Sending raw message"
        );
        self.framed.send(raw).await?;
        Ok(())
    }

    pub fn peer_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.framed.get_ref().peer_addr()
    }
}
