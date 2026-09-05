use crate::state::AppState;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

pub async fn serve(
    state: Arc<AppState>,
    bind: &str,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(bind).await?;

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (stream, _) = result?;
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(err) = handle_connection(stream, state).await {
                        tracing::debug!("health connection failed: {err}");
                    }
                });
            }
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() {
                    return Ok(());
                }
            }
        }
    }
}

async fn handle_connection(mut stream: TcpStream, state: Arc<AppState>) -> anyhow::Result<()> {
    let mut request = [0_u8; 1024];
    let length = stream.read(&mut request).await?;
    let first_line = std::str::from_utf8(&request[..length])?
        .lines()
        .next()
        .unwrap_or_default();
    let path = first_line.split_whitespace().nth(1).unwrap_or("/");

    let (status, content_type, body) = match path {
        "/live" => ("200 OK", "text/plain", "ok\n".to_string()),
        "/ready" => {
            let ready = state.accepting.load(Ordering::Relaxed)
                && state.redis_connected.load(Ordering::Relaxed);
            if ready {
                ("200 OK", "text/plain", "ready\n".to_string())
            } else {
                (
                    "503 Service Unavailable",
                    "text/plain",
                    "not ready\n".to_string(),
                )
            }
        }
        "/metrics" => ("200 OK", "text/plain; version=0.0.4", state.metrics()),
        _ => ("404 Not Found", "text/plain", "not found\n".to_string()),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}
