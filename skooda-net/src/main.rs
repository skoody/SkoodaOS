mod config;
mod dhcp;
mod dns;
mod interface;
mod monitor;
mod wifi;
mod route;

use crate::config::{load_config, NetworkConfig};
use crate::monitor::{LinkMonitor, LinkState};
use crate::wifi::WifiManager;
use skooda_utils::logging::init_logging;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info, warn};

struct NetworkState {
    active_default_iface: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Daemonize: run in background, keep current dir, close stdio (redirect to /dev/null)
    // We will re-open /dev/kmsg for logging in init_logging anyway.
    if let Err(e) = nix::unistd::daemon(false, false) {
        eprintln!("Failed to daemonize: {}", e);
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
    // Prioritization: eth* > wlan*
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
                // Same type or other, update if new
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
