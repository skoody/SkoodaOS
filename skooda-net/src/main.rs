mod config;
mod dhcp;
mod dns;
mod interface;
mod monitor;
mod wifi;
mod route;
mod ipc;

use clap::{Parser, Subcommand};
use crate::config::{load_config, NetworkConfig};
use crate::monitor::{LinkMonitor, LinkState};
use crate::wifi::WifiManager;
use crate::ipc::{IpcCommand, IpcResponse, start_ipc_server, send_ipc_command};
use skooda_utils::logging::init_logging;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info, warn};

#[derive(Parser)]
#[command(name = "skooda-net", version = "0.2", about = "SkoodaOS Network Manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the network daemon
    Daemon {
        /// Run in background
        #[arg(short, long)]
        background: bool,
    },
    /// Manage WiFi
    Wifi {
        #[command(subcommand)]
        cmd: WifiCommands,
    },
    /// Show status
    Status,
}

#[derive(Subcommand)]
enum WifiCommands {
    /// Scan for networks
    Scan,
    /// Connect to a network
    Connect { ssid: String, psk: String },
}

struct NetworkState {
    active_default_iface: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon { background } => run_daemon(background).await,
        Commands::Wifi { cmd } => handle_cli_wifi(cmd).await,
        Commands::Status => handle_cli_status().await,
    }
}

async fn handle_cli_wifi(cmd: WifiCommands) -> anyhow::Result<()> {
    match cmd {
        WifiCommands::Scan => {
            println!("Scanning for WiFi networks...");
            match send_ipc_command(IpcCommand::WifiScan).await? {
                IpcResponse::ScanResults(ssids) => {
                    for ssid in ssids {
                        println!(" - {}", ssid);
                    }
                }
                IpcResponse::Error(e) => eprintln!("Error: {}", e),
                _ => eprintln!("Unexpected response"),
            }
        }
        WifiCommands::Connect { ssid, psk } => {
            println!("Connecting to {}...", ssid);
            match send_ipc_command(IpcCommand::WifiConnect { ssid, psk }).await? {
                IpcResponse::Success(msg) => println!("Success: {}", msg),
                IpcResponse::Error(e) => eprintln!("Error: {}", e),
                _ => eprintln!("Unexpected response"),
            }
        }
    }
    Ok(())
}

async fn handle_cli_status() -> anyhow::Result<()> {
    match send_ipc_command(IpcCommand::Status).await? {
        IpcResponse::Status { active_interface, ip_address } => {
            println!("Active Interface: {}", active_interface.unwrap_or_else(|| "None".into()));
            println!("IP Address:       {}", ip_address.unwrap_or_else(|| "None".into()));
        }
        IpcResponse::Error(e) => eprintln!("Error: {}", e),
        _ => eprintln!("Unexpected response"),
    }
    Ok(())
}

async fn run_daemon(background: bool) -> anyhow::Result<()> {
    if background {
        if let Err(e) = nix::unistd::daemon(false, false) {
            eprintln!("Failed to daemonize: {}", e);
        }
    }

    init_logging();
    info!("Starting SkoodaOS Network Daemon v0.2...");

    let config = Arc::new(load_config().unwrap_or_else(|e| {
        error!("Config error: {}. Using empty config.", e);
        NetworkConfig {
            interfaces: std::collections::HashMap::new(),
            wifi_networks: Vec::new(),
        }
    }));

    let state = Arc::new(Mutex::new(NetworkState {
        active_default_iface: None,
    }));

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    // Create IPC Server Handler
    let state_clone = state.clone();
    let handler = move |cmd: IpcCommand| {
        let state = state_clone.clone();
        async move {
            match cmd {
                IpcCommand::Status => {
                    let s = state.lock().await;
                    IpcResponse::Status {
                        active_interface: s.active_default_iface.clone(),
                        ip_address: None, // Simplified for now
                    }
                }
                IpcCommand::WifiScan => {
                    // For now, hardcode "wlan0" or find active wlan
                    let wmgr = WifiManager::new("wlan0".to_string(), vec![]);
                    match wmgr.scan() {
                        Ok(ssids) => IpcResponse::ScanResults(ssids),
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                }
                IpcCommand::WifiConnect { ssid, psk } => {
                    let wmgr = WifiManager::new("wlan0".to_string(), vec![]);
                    match wmgr.connect(&ssid, &psk).await {
                        Ok(_) => IpcResponse::Success("Connected".into()),
                        Err(e) => IpcResponse::Error(e.to_string()),
                    }
                }
            }
        }
    };

    start_ipc_server(handler).await?;

    // Start WiFi Managers for wlan interfaces
    for (name, _) in &config.interfaces {
        if name.starts_with("wlan") {
            let wifi_mgr = WifiManager::new(name.clone(), config.wifi_networks.clone());
            tokio::spawn(async move {
                wifi_mgr.run_loop().await;
            });
        }
    }

    let mut monitors: Vec<(String, LinkMonitor)> = config.interfaces.keys()
        .map(|name| (name.clone(), LinkMonitor::new(name.clone())))
        .collect();

    info!("Monitoring interfaces: {:?}", config.interfaces.keys().collect::<Vec<_>>());

    loop {
        tokio::select! {
            _ = sigterm.recv() => break,
            _ = sigint.recv() => break,
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                for (name, monitor) in &mut monitors {
                    if let Some(link_state) = monitor.check() {
                        handle_link_change(name, link_state, &config, &state).await;
                    }
                    if let Some(signal) = monitor.get_wifi_signal() {
                        info!("[net] WiFi {} Signal Strength: {} dBm", name, signal);
                    }
                }
            }
        }
    }

    info!("Shutting down SkoodaOS Network Daemon...");
    let _ = std::fs::remove_file(ipc::IPC_SOCKET_PATH);
    Ok(())
}

async fn handle_link_change(
    iface: &str, 
    state: LinkState, 
    config: &NetworkConfig, 
    net_state: &Arc<Mutex<NetworkState>>
) {
    match state {
        LinkState::Up => {
            info!("[net] Link UP on {}", iface);
            if let Some(cfg) = config.interfaces.get(iface) {
                if cfg.dhcp {
                    let iface_name = iface.to_string();
                    let net_state_clone = net_state.clone();
                    tokio::spawn(async move {
                        match dhcp::dhcp_request(&iface_name).await {
                            Ok(lease) => {
                                info!("[net] DHCP success for {}: {}", iface_name, lease.ip);
                                let _ = dns::write_resolv_conf(lease.dns);
                                
                                let mut s = net_state_clone.lock().await;
                                update_routing(&iface_name, lease.gateway, &mut s).await;
                            }
                            Err(e) => error!("[net] DHCP failed for {}: {}", iface_name, e),
                        }
                    });
                }
            }
        }
        LinkState::Down => {
            warn!("[net] Link DOWN on {}", iface);
            let mut s = net_state.lock().await;
            if s.active_default_iface.as_deref() == Some(iface) {
                info!("[net] Removing default route from {}", iface);
                let _ = route::delete_default_route(iface);
                s.active_default_iface = None;
            }
        }
        _ => {}
    }
}

async fn update_routing(iface: &str, gateway: std::net::Ipv4Addr, state: &mut NetworkState) {
    let should_update = match &state.active_default_iface {
        None => true,
        Some(active) => {
            if iface.starts_with("eth") && active.starts_with("wlan") {
                info!("[net] Prioritizing Ethernet ({}) over WiFi ({})", iface, active);
                let _ = route::delete_default_route(active);
                true
            } else if iface.starts_with("wlan") && active.starts_with("eth") {
                info!("[net] Ethernet ({}) is already active, keeping it over {}", active, iface);
                false
            } else {
                true
            }
        }
    };

    if should_update {
        if let Err(e) = route::add_default_route(iface, gateway) {
            error!("[net] Failed to set default route for {}: {}", iface, e);
        } else {
            state.active_default_iface = Some(iface.to_string());
        }
    }
}
