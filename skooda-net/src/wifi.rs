use std::process::Command;
use std::fs;
use skooda_utils::error::{Result, SkoodaError};
use tracing::info;
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

    fn is_connected(&self) -> bool {
        let output = Command::new("/bin/wpa_cli")
            .args(["-i", &self.interface, "status"])
            .output();
        
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            return text.contains("wpa_state=COMPLETED");
        }
        false
    }

    fn scan(&self) -> Result<Vec<String>> {
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
        let conf = format!(
            "ctrl_interface=/var/run/wpa_supplicant\nupdate_config=1\n\nnetwork={{\n    ssid=\"{}\"\n    psk=\"{}\"\n}}\n",
            ssid, psk
        );
        
        fs::write("/etc/wpa_supplicant.conf", conf).map_err(|e| SkoodaError::Io {
            path: "/etc/wpa_supplicant.conf".into(),
            source: e,
        })?;

        let _ = Command::new("/bin/wpa_cli").args(["terminate"]).status();
        std::thread::sleep(std::time::Duration::from_millis(500));

        let status = Command::new("/bin/wpa_supplicant")
            .args(["-B", "-i", &self.interface, "-c", "/etc/wpa_supplicant.conf", "-D", "nl80211"])
            .status()
            .map_err(|e| SkoodaError::System(format!("Failed to start wpa_supplicant: {}", e)))?;

        if !status.success() {
            return Err(SkoodaError::Network("wpa_supplicant failed to start".into()));
        }

        Ok(())
    }
}
