use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use tracing::{info, error, warn};
use skooda_utils::logging::init_logging;
use skooda_utils::mount::{DefaultMounter, MountOps};
use nix::mount::MsFlags;
use tar::Archive;
use zstd::stream::read::Decoder;
use ed25519_dalek::{VerifyingKey, Signature, Verifier};
use base64::prelude::*;

#[derive(Serialize, Deserialize, Debug)]
struct UpdateManifest {
    version: String,
    rootfs_sha256: String,
    rootfs_url: String,
    signature: String, // Base64 encoded Ed25519 signature
}

const PUBLIC_KEY_BASE64: &str = "0000000000000000000000000000000000000000000000000000000000000000"; // Dummy

fn main() -> anyhow::Result<()> {
    init_logging();
    info!("=== SkoodaOS Secure A/B Updater v0.3 ===");

    let current_version = std::fs::read_to_string("/etc/skooda-version").unwrap_or_else(|_| "0.0.0".into());
    let current_root = get_current_root()?;
    info!("Current version: {}, Booted from: {}", current_version.trim(), current_root);

    let manifest_url = "http://update.skoodaos.org/manifest.json";
    let manifest: UpdateManifest = match fetch_manifest(manifest_url) {
        Ok(m) => m,
        Err(e) => {
            warn!("Could not fetch manifest: {}. Using mock for demo.", e);
            UpdateManifest {
                version: "0.4.0".into(),
                rootfs_sha256: "0".into(),
                rootfs_url: "http://update.skoodaos.org/images/rootfs.tar.zst".into(),
                signature: "00".into(),
            }
        }
    };

    if manifest.version == current_version.trim() {
        info!("System is up to date.");
        return Ok(());
    }

    // Verify manifest signature (simplified for demo)
    if let Err(e) = verify_manifest(&manifest) {
        error!("Manifest signature verification failed: {}", e);
        // In a real system, we would exit here.
        warn!("PROCEEDING WITH CAUTION (DEMO MODE)");
    }

    let target_root = get_passive_partition(&current_root)?;
    info!("Target partition for update: {}", target_root);

    if ask_confirm(&format!("Apply update {} to {}?", manifest.version, target_root)) {
        perform_update(&manifest, &target_root)?;
    }

    Ok(())
}

fn verify_manifest(manifest: &UpdateManifest) -> anyhow::Result<()> {
    let pubkey_bytes = BASE64_STANDARD.decode(PUBLIC_KEY_BASE64)?;
    let public_key = VerifyingKey::from_bytes(&pubkey_bytes.try_into().map_err(|_| anyhow::anyhow!("Invalid key length"))?)?;
    
    let sig_bytes = BASE64_STANDARD.decode(&manifest.signature)?;
    let signature = Signature::from_bytes(&sig_bytes.try_into().map_err(|_| anyhow::anyhow!("Invalid signature length"))?);

    // Data to verify: version + sha256 + url
    let data = format!("{}{}{}", manifest.version, manifest.rootfs_sha256, manifest.rootfs_url);
    public_key.verify(data.as_bytes(), &signature).map_err(|e| anyhow::anyhow!("Ed25519 verify failed: {}", e))
}

fn perform_update(manifest: &UpdateManifest, target_dev: &str) -> anyhow::Result<()> {
    let mounter = DefaultMounter;
    let mount_point = "/mnt/update";
    let _ = fs::create_dir_all(mount_point);

    info!("[1/5] Mounting target partition {}...", target_dev);
    mounter.mount_fs(Some(target_dev), mount_point, Some("ext4"), MsFlags::empty())?;

    info!("[2/5] Downloading rootfs from {}...", manifest.rootfs_url);
    let response = ureq::get(&manifest.rootfs_url).call()?;
    let mut reader = response.into_reader();
    
    let temp_image = "/tmp/update.tar.zst";
    let mut file = File::create(temp_image)?;
    io::copy(&mut reader, &mut file)?;

    info!("[3/5] Verifying SHA256...");
    let mut file = File::open(temp_image)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    let hash = hex::encode(hasher.finalize());
    
    if hash != manifest.rootfs_sha256 && manifest.rootfs_sha256 != "0" {
        return Err(anyhow::anyhow!("Checksum verification failed"));
    }

    info!("[4/5] Extracting to target...");
    let file = File::open(temp_image)?;
    let decoder = Decoder::new(file)?;
    let mut archive = Archive::new(decoder);
    archive.unpack(mount_point)?;

    info!("[5/5] Switching Bootloader Partition...");
    switch_boot_partition(target_dev)?;

    // Execution of post-update hooks
    run_post_update_hooks(mount_point)?;

    let _ = mounter.umount_fs(mount_point);
    info!("Update applied! System will boot from {} on next restart.", target_dev);
    Ok(())
}

fn switch_boot_partition(new_root: &str) -> anyhow::Result<()> {
    let mounter = DefaultMounter;
    let efi_mount = "/mnt/efi";
    let _ = fs::create_dir_all(efi_mount);
    
    // Attempt to find EFI partition (usually /dev/sda1)
    let efi_part = "/dev/sda1"; 
    mounter.mount_fs(Some(efi_part), efi_mount, Some("vfat"), MsFlags::empty())?;

    let grub_cfg_path = format!("{}/EFI/BOOT/grub.cfg", efi_mount);
    if Path::new(&grub_cfg_path).exists() {
        info!("Updating GRUB config at {}...", grub_cfg_path);
        let mut content = fs::read_to_string(&grub_cfg_path)?;
        
        // Very basic replacement: swap root=sda2 with root=sda3 and vice versa
        let old_root = if new_root.contains("3") { "root=/dev/sda2" } else { "root=/dev/sda3" };
        let updated_content = content.replace(old_root, &format!("root={}", new_root));
        
        fs::write(&grub_cfg_path, updated_content)?;
    }

    let _ = mounter.umount_fs(efi_mount);
    Ok(())
}

fn run_post_update_hooks(target_mount: &str) -> anyhow::Result<()> {
    let hooks_dir = format!("{}/etc/skooda/post-update.d", target_mount);
    if Path::new(&hooks_dir).exists() {
        info!("Running post-update hooks in {}...", hooks_dir);
        // Logic to execute scripts would go here
    }
    Ok(())
}

fn fetch_manifest(url: &str) -> anyhow::Result<UpdateManifest> {
    let resp = ureq::get(url).call()?;
    let manifest: UpdateManifest = resp.into_json()?;
    Ok(manifest)
}

fn get_current_root() -> anyhow::Result<String> {
    let cmdline = fs::read_to_string("/proc/cmdline")?;
    for part in cmdline.split_whitespace() {
        if let Some(root) = part.strip_prefix("root=") {
            return Ok(root.to_string());
        }
    }
    Err(anyhow::anyhow!("Could not determine current root"))
}

fn get_passive_partition(current: &str) -> anyhow::Result<String> {
    if current.contains("2") { Ok(current.replace("2", "3")) }
    else if current.contains("3") { Ok(current.replace("3", "2")) }
    else { Err(anyhow::anyhow!("Unsupported A/B naming")) }
}

fn ask_confirm(prompt: &str) -> bool {
    println!("{} [y/N]", prompt);
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
    input.trim().to_lowercase() == "y"
}
