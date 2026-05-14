use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tracing::{error, info};

pub const IPC_SOCKET_PATH: &str = "/var/run/skooda-net.sock";

#[derive(Debug, Serialize, Deserialize)]
pub enum IpcCommand {
    Status,
    WifiScan,
    WifiConnect { ssid: String, psk: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum IpcResponse {
    Status {
        active_interface: Option<String>,
        ip_address: Option<String>,
    },
    ScanResults(Vec<String>),
    Success(String),
    Error(String),
}

/// Startet den IPC-Server im Daemon
pub async fn start_ipc_server<F, Fut>(handler: F) -> anyhow::Result<()>
where
    F: Fn(IpcCommand) -> Fut + Send + Sync + 'static + Clone,
    Fut: std::future::Future<Output = IpcResponse> + Send + 'static,
{
    let path = Path::new(IPC_SOCKET_PATH);
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }

    let listener = UnixListener::bind(path)?;
    // Setze Permissions für den Socket
    let _ = std::process::Command::new("chmod")
        .args(["666", IPC_SOCKET_PATH])
        .status();

    info!("IPC Server listening on {}", IPC_SOCKET_PATH);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((mut socket, _)) => {
                    let handler = handler.clone();
                    tokio::spawn(async move {
                        let mut buf = vec![0u8; 4096];
                        if let Ok(n) = socket.read(&mut buf).await {
                            if n == 0 {
                                return;
                            }
                            if let Ok(command) = serde_json::from_slice::<IpcCommand>(&buf[..n]) {
                                let response = handler(command).await;
                                if let Ok(res_bytes) = serde_json::to_vec(&response) {
                                    let _ = socket.write_all(&res_bytes).await;
                                }
                            } else {
                                let err = IpcResponse::Error("Invalid JSON command".into());
                                let _ = socket.write_all(&serde_json::to_vec(&err).unwrap()).await;
                            }
                        }
                    });
                }
                Err(e) => error!("Failed to accept IPC connection: {}", e),
            }
        }
    });

    Ok(())
}

/// Sendet ein Kommando von der CLI an den Daemon
pub async fn send_ipc_command(command: IpcCommand) -> anyhow::Result<IpcResponse> {
    let path = Path::new(IPC_SOCKET_PATH);
    if !path.exists() {
        return Err(anyhow::anyhow!("Daemon socket not found at {}", IPC_SOCKET_PATH));
    }

    let mut stream = UnixStream::connect(path).await?;
    let cmd_bytes = serde_json::to_vec(&command)?;
    stream.write_all(&cmd_bytes).await?;

    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Err(anyhow::anyhow!("Daemon closed connection unexpectedly"));
    }

    let response: IpcResponse = serde_json::from_slice(&buf[..n])?;
    Ok(response)
}
