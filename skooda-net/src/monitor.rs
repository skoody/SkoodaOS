use std::path::Path;
use std::fs;

#[derive(Debug, PartialEq, Clone)]
pub enum LinkState {
    Up,
    Down,
    Unknown,
}

pub struct LinkMonitor {
    interface: String,
    last_state: LinkState,
}

impl LinkMonitor {
    pub fn new(interface: String) -> Self {
        Self {
            interface,
            last_state: LinkState::Unknown,
        }
    }

    pub fn check(&mut self) -> Option<LinkState> {
        let state = self.get_carrier_state();
        if state != self.last_state {
            self.last_state = state.clone();
            Some(state)
        } else {
            None
        }
    }

    fn get_carrier_state(&self) -> LinkState {
        let path = format!("/sys/class/net/{}/carrier", self.interface);
        if !Path::new(&path).exists() {
            // Check if it's up via operstate if carrier is missing
            let oper_path = format!("/sys/class/net/{}/operstate", self.interface);
            if let Ok(state) = fs::read_to_string(oper_path) {
                if state.trim() == "up" { return LinkState::Up; }
            }
            return LinkState::Down;
        }

        match fs::read_to_string(path) {
            Ok(content) if content.trim() == "1" => LinkState::Up,
            _ => LinkState::Down,
        }
    }

    pub fn get_wifi_signal(&self) -> Option<i32> {
        if !self.interface.starts_with("wlan") { return None; }
        
        let path = format!("/proc/net/wireless");
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                if line.contains(&self.interface) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() > 3 {
                        // Level is usually the 4th column
                        return parts[3].parse::<f32>().ok().map(|v| v as i32);
                    }
                }
            }
        }
        None
    }
}
