use futures_util::stream::StreamExt;
use nix::mount::MsFlags;
use nix::unistd::{chdir, chroot, execvp, getpid};
use serde::Deserialize;
use skooda_utils::logging::init_logging;
use skooda_utils::mount::{DefaultMounter, MountOps};
use skooda_utils::error::{Result, SkoodaError};
use std::ffi::CString;
use std::path::Path;
use std::process::{Command, Stdio};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

#[derive(Debug, Deserialize, Clone)]
enum RestartPolicy {
    Always,
    OnFailure,
    Never,
}

#[derive(Debug, Deserialize, Clone)]
struct ServiceConfig {
    name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default = "default_restart_policy")]
    restart_policy: RestartPolicy,
    #[serde(default)]
    _dependencies: Vec<String>,
}

fn default_restart_policy() -> RestartPolicy {
    RestartPolicy::Always
}

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(default)]
    services: Vec<ServiceConfig>,
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let pid = getpid();
    
    // Phase 1: Early Init
    let mounter = DefaultMounter;
    early_setup(&mounter);
    init_logging();
    
    debug!("=== SkoodaOS Init v0.2.5 (Fully Dynamic) ===");

    // Phase 2: Hardware Detection & RootFS
    if pid.as_raw() == 1 {
        // Coldplug Devices
        info!("Scanning hardware (mdev)...");
        let _ = Command::new("/bin/mdev").arg("-s").status();
        
        load_critical_modules();
        
        // Auto-load drivers based on modalias
        info!("Auto-loading drivers...");
        let _ = auto_load_modules();
        
        bring_up_interfaces();

        if Path::new("/etc/skoodaos-initramfs").exists() {
            if let Some(root_dev) = get_root_device() {
                switch_to_root(&mounter, &root_dev);
            }
        }
    }

    // Phase 3: Service Management
    let config = load_config("/etc/services.toml");
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    
    for svc in config.services {
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            run_service(svc, tx_clone).await;
        });
    }

    let mut signals = signal_hook_tokio::Signals::new(&[signal_hook::consts::SIGCHLD])?;
    
    loop {
        tokio::select! {
            Some(sig) = signals.next() => {
                if sig == signal_hook::consts::SIGCHLD {
                    reap_zombies();
                }
            }
            Some(msg) = rx.recv() => {
                debug!("Service Manager: {}", msg);
            }
        }
    }
}

fn load_config(path: &str) -> Config {
    if Path::new(path).exists() {
        match std::fs::read_to_string(path) {
            Ok(content) => match toml::from_str::<Config>(&content) {
                Ok(c) => return c,
                Err(e) => error!("Failed to parse {}: {}", path, e),
            },
            Err(e) => error!("Failed to read {}: {}", path, e),
        }
    }
    get_fallback_config()
}

fn get_fallback_config() -> Config {
    Config { services: vec![
        ServiceConfig {
            name: "shell".to_string(),
            command: "/bin/skooda-sh".to_string(),
            args: vec![],
            restart_policy: RestartPolicy::Always,
            _dependencies: vec![],
        }
    ]}
}

async fn run_service(service: ServiceConfig, tx: mpsc::UnboundedSender<String>) {
    loop {
        let name = service.name.clone();
        debug!("Starting service: {}", name);
        
        let mut child = match tokio::process::Command::new(&service.command)
            .args(&service.args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn() 
        {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to spawn service {}: {}", name, e);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let status = child.wait().await;
        let exit_code = status.as_ref().map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
        
        if exit_code != 0 {
            warn!("Service {} exited with status: {:?}", name, status);
        }

        match service.restart_policy {
            RestartPolicy::Always => {
                let _ = tx.send(format!("Restarting service {}", name));
            }
            RestartPolicy::OnFailure if exit_code != 0 => {
                let _ = tx.send(format!("Restarting service {} (Failure)", name));
            }
            _ => {
                debug!("Service {} finished", name);
                break;
            }
        }
        
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

fn reap_zombies() {
    unsafe {
        while libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) > 0 {}
    }
}

fn get_root_device() -> Option<String> {
    let cmdline = std::fs::read_to_string("/proc/cmdline").ok()?;
    for param in cmdline.split_whitespace() {
        if let Some(dev) = param.strip_prefix("root=") {
            return Some(dev.to_string());
        }
    }
    None
}

fn switch_to_root(mounter: &DefaultMounter, root_dev: &str) {
    for _ in 0..15 {
        if Path::new(root_dev).exists() { break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let _ = std::fs::create_dir_all("/mnt/root");
    if let Err(e) = mounter.mount_fs(Some(root_dev), "/mnt/root", Some("ext4"), MsFlags::empty()) {
        error!("Mount root failed: {}", e);
        return;
    }

    for mnt in ["/dev", "/proc", "/sys", "/run", "/tmp"] {
        let dest = format!("/mnt/root{}", mnt);
        let _ = std::fs::create_dir_all(&dest);
        let _ = mounter.mount_fs(Some(mnt), &dest, None, MsFlags::MS_MOVE);
    }

    let _ = chdir("/mnt/root");
    let _ = mounter.mount_fs(Some("/mnt/root"), "/", None, MsFlags::MS_MOVE);
    let _ = chroot(".");
    let _ = chdir("/");

    let init_candidates = ["/bin/init", "/sbin/init", "/init"];
    for path in init_candidates {
        if Path::new(path).exists() {
            if let Ok(init) = CString::new(path) {
                let _ = execvp(&init, &[init.clone()]);
            }
        }
    }
}

fn auto_load_modules() -> Result<()> {
    // Only scan PCI and USB buses to avoid ACPI/CPU spam
    for bus in ["/sys/bus/pci/devices", "/sys/bus/usb/devices"] {
        if Path::new(bus).exists() {
            let _ = Command::new("/bin/busybox")
                .args(["find", bus, "-name", "modalias", "-exec", "modprobe", "-qab", "{}", "+"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    Ok(())
}

fn load_critical_modules() -> Result<()> {
    let critical = ["vmd", "nvme", "usb-storage", "uas", "fat", "vfat", "xhci-pci", "xhci-hcd"];
    for mod_name in critical {
        let _ = Command::new("/bin/modprobe").arg(mod_name).status();
    }
    Ok(())
}

fn early_setup(mounter: &DefaultMounter) {
    let mounts = [
        ("proc", "/proc", "proc", MsFlags::empty()),
        ("sysfs", "/sys", "sysfs", MsFlags::empty()),
        ("devtmpfs", "/dev", "devtmpfs", MsFlags::empty()),
        ("tmpfs", "/run", "tmpfs", MsFlags::empty()),
        ("tmpfs", "/tmp", "tmpfs", MsFlags::empty()),
    ];

    for (source, target, fstype, flags) in mounts {
        let _ = std::fs::create_dir_all(target);
        let _ = mounter.mount_fs(Some(source), target, Some(fstype), flags);
    }
}

fn bring_up_interfaces() {
    info!("Bringing up network interfaces...");
    // Give drivers a moment to register net devices
    std::thread::sleep(std::time::Duration::from_millis(500));
    
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if name != "lo" {
                    info!("Bringing up {}", name);
                    let _ = Command::new("/bin/busybox")
                        .args(["ip", "link", "set", &name, "up"])
                        .status();
                }
            }
        }
    }
}
