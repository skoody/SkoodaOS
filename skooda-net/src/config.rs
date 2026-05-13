use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct WifiConfig {
    pub ssid: String,
    pub psk: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InterfaceConfig {
    #[serde(default)]
    pub dhcp: bool,
    pub _static_ip: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NetworkConfig {
    #[serde(default)]
    pub interfaces: HashMap<String, InterfaceConfig>,
    #[serde(default)]
    pub wifi_networks: Vec<WifiConfig>,
}

pub fn load_config() -> anyhow::Result<NetworkConfig> {
    let path = "/etc/network.toml";
    if std::path::Path::new(path).exists() {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    } else {
        Ok(NetworkConfig {
            interfaces: HashMap::new(),
            wifi_networks: Vec::new(),
        })
    }
}
