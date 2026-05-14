use std::process::Command;
use std::fs;
use skooda_utils::error::{Result, SkoodaError};
use tracing::{info, warn, error};
use crate::config::WifiConfig;

pub struct WifiManager {
    interface: String,
    known_networks: Vec<WifiConfig>,
}

impl WifiManager {
    pub fn new(interface: String, known_networks: Vec<WifiConfig>) -> Self {
        Self { interface, known_networks }
    }

    pub async fn run_loop(&self) {
        info!("[wifi] Starting WiFi manager for {}...", self.interface);
        
        self.ensure_supplicant_running();

        loop {
            if !self.is_connected() {
                info!("[wifi] Not connected, scanning...");
                if let Ok(ssids) = self.scan() {
                    for network in &self.known_networks {
                        if ssids.contains(&network.ssid) {
                            info!("[wifi] Found known network: {}, connecting...", network.ssid);
                            let _ = self.connect(&network.ssid, &network.psk).await;
                            break;
                        }
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    }

    pub fn ensure_supplicant_running(&self) {
        let status = Command::new("/bin/wpa_cli")
            .args(["-i", &self.interface, "ping"])
            .output();

        let running = if let Ok(out) = status {
            String::from_utf8_lossy(&out.stdout).contains("PONG")
        } else {
            false
        };

        if !running {
            info!("[wifi] Starting wpa_supplicant for {}", self.interface);
            
            // Ensure config exists
            if !std::path::Path::new("/etc/wpa_supplicant.conf").exists() {
                let _ = fs::write("/etc/wpa_supplicant.conf", "ctrl_interface=/var/run/wpa_supplicant\nupdate_config=1\n");
            }

            let _ = Command::new("/bin/wpa_supplicant")
                .args(["-B", "-i", &self.interface, "-c", "/etc/wpa_supplicant.conf", "-D", "nl80211"])
                .status();
                
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    pub fn is_connected(&self) -> bool {
        let output = Command::new("/bin/wpa_cli")
            .args(["-i", &self.interface, "status"])
            .output();
        
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            return text.contains("wpa_state=COMPLETED");
        }
        false
    }

    pub fn scan(&self) -> Result<Vec<String>> {
        self.ensure_supplicant_running();
        
        let _ = Command::new("/bin/wpa_cli").args(["-i", &self.interface, "scan"]).status();
        std::thread::sleep(std::time::Duration::from_secs(2));
        
        let output = Command::new("/bin/wpa_cli")
            .args(["-i", &self.interface, "scan_results"])
            .output()
            .map_err(|e| SkoodaError::System(format!("scan_results failed: {}", e)))?;

        let text = String::from_utf8_lossy(&output.stdout);
        let mut ssids = Vec::new();
        for line in text.lines().skip(1) {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 5 {
                ssids.push(parts[4].to_string());
            }
        }
        Ok(ssids)
    }

    pub async fn connect(&self, ssid: &str, psk: &str) -> Result<()> {
        self.ensure_supplicant_running();

        // 1. Add network
        let add_out = Command::new("/bin/wpa_cli")
            .args(["-i", &self.interface, "add_network"])
            .output()
            .map_err(|e| SkoodaError::System(format!("wpa_cli add_network failed: {}", e)))?;
            
        let net_id = String::from_utf8_lossy(&add_out.stdout).trim().to_string();
        if net_id.is_empty() || net_id.contains("FAIL") {
            return Err(SkoodaError::Network(format!("Failed to add network: {}", net_id)));
        }

        // 2. Set SSID
        let _ = Command::new("/bin/wpa_cli")
            .args(["-i", &self.interface, "set_network", &net_id, "ssid", &format!("\"{}\"", ssid)])
            .status();

        // 3. Set PSK
        let _ = Command::new("/bin/wpa_cli")
            .args(["-i", &self.interface, "set_network", &net_id, "psk", &format!("\"{}\"", psk)])
            .status();

        // 4. Enable network
        let _ = Command::new("/bin/wpa_cli")
            .args(["-i", &self.interface, "enable_network", &net_id])
            .status();

        // 5. Save config
        let _ = Command::new("/bin/wpa_cli")
            .args(["-i", &self.interface, "save_config"])
            .status();

        Ok(())
    }
}
