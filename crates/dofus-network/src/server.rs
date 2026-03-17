use crate::session::Session;
use std::future::Future;
use tokio::net::TcpListener;
use tracing;

/// Generic TCP server that accepts connections and spawns a handler per client.
pub async fn run_server<F, Fut>(addr: &str, handler: F) -> anyhow::Result<()>
where
    F: Fn(Session) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Listening on {addr}");

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        tracing::info!(%peer_addr, "New connection");

        let session = Session::new(stream);
        let fut = handler(session);

        tokio::spawn(async move {
            if let Err(e) = fut.await {
                tracing::error!(%peer_addr, error = %e, "Session error");
            }
            tracing::info!(%peer_addr, "Connection closed");
        });
    }
}
