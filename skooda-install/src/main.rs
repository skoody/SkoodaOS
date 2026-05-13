use dialoguer::{Confirm, Select};
use std::fs::{self, File};
use std::process::Command;
use tracing::{info, error, warn};
use skooda_utils::logging::init_logging;
use skooda_utils::mount::{DefaultMounter, MountOps};
use nix::mount::MsFlags;
use tar::Archive;
use zstd::stream::read::Decoder;
use std::path::Path;

fn main() {
    init_logging();
    let args: Vec<String> = std::env::args().collect();
    let debug = args.contains(&"-debug".to_string()) || args.contains(&"--debug".to_string());

    info!("=== SkoodaOS Installer v0.2 (Image-based) ===");

    let disks = get_disks(debug);
    if disks.is_empty() {
        error!("No suitable disks found!");
        return;
    }

    let selection = Select::new()
        .with_prompt("Select target disk for SkoodaOS installation")
        .items(&disks)
        .default(0)
        .interact()
        .unwrap();

    let target = &disks[selection];

    if Confirm::new()
        .with_prompt(format!("WARNING: All data on {} will be destroyed. Continue?", target))
        .interact()
        .unwrap()
    {
        if let Err(e) = install_to(target) {
            error!("Installation failed: {}", e);
        }
    } else {
        info!("Installation cancelled.");
    }
}

fn get_disks(debug: bool) -> Vec<String> {
    let mut disks = Vec::new();
    if !Path::new("/sys/block").exists() { return disks; }

    if let Ok(entries) = fs::read_dir("/sys/block") {
        for entry in entries.flatten() {
            let name = entry.file_name().into_string().unwrap();
            if (name.starts_with("sd") || name.starts_with("nvme") || name.starts_with("mmcblk")) 
               && !name.contains('p') && !name.contains("loop") && !name.contains("ram") 
            {
                let removable_path = format!("/sys/block/{}/removable", name);
                if let Ok(content) = fs::read_to_string(&removable_path) {
                    if content.trim() == "1" { continue; }
                }

                let path = format!("/dev/{}", name);
                disks.push(path);
            }
        }
    }
    disks
}

fn install_to(target: &str) -> anyhow::Result<()> {
    let mounter = DefaultMounter;

    info!("[1/4] Partitioning {}...", target);
    let sfdisk_input = "label: gpt\nsize=512M, type=U, bootable\nsize=+, type=L\n";
    let mut child = Command::new("/bin/sfdisk")
        .arg("--wipe")
        .arg("always")
        .arg(target)
        .stdin(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(sfdisk_input.as_bytes())?;
    }

    if !child.wait()?.success() {
        return Err(anyhow::anyhow!("Partitioning failed"));
    }
    
    info!("[2/4] Formatting partitions...");
    let (efi_part, root_part) = if target.contains("nvme") || target.contains("mmcblk") {
        (format!("{}p1", target), format!("{}p2", target))
    } else {
        (format!("{}1", target), format!("{}2", target))
    };

    if !Command::new("/bin/mkfs.vfat").arg("-F32").arg(&efi_part).status()?.success() {
        return Err(anyhow::anyhow!("Formatting EFI failed"));
    }
    
    if !Command::new("/bin/mkfs.ext4").arg("-F").arg(&root_part).status()?.success() {
        return Err(anyhow::anyhow!("Formatting Root failed"));
    }

    info!("[3/4] Extracting SkoodaOS RootFS image...");
    let target_mount = "/mnt/target";
    let _ = fs::create_dir_all(target_mount);
    
    mounter.mount_fs(Some(root_part.as_str()), target_mount, Some("ext4"), MsFlags::empty())?;

    let image_path = "/rootfs.tar.zst";
    if Path::new(image_path).exists() {
        let file = File::open(image_path)?;
        let decoder = Decoder::new(file)?;
        let mut archive = Archive::new(decoder);
        archive.unpack(target_mount)?;
    } else {
        warn!("{} not found, falling back to manual copy (slower)", image_path);
        let copy_dirs = ["/bin", "/etc", "/lib", "/sbin", "/var", "/boot", "/usr"];
        for dir in copy_dirs.iter() {
            let dest = format!("{}{}", target_mount, dir);
            let _ = Command::new("/bin/cp").arg("-ax").arg(dir).arg(dest).status();
        }
    }

    for dir in ["/dev", "/proc", "/sys", "/mnt", "/run", "/tmp"] {
        let _ = fs::create_dir_all(format!("{}/{}", target_mount, dir));
    }

    let _ = fs::remove_file(format!("{}/etc/skoodaos-initramfs", target_mount));

    info!("[4/4] Installing Bootloader...");
    let efi_mount = format!("{}/boot/efi", target_mount);
    let _ = fs::create_dir_all(&efi_mount);
    mounter.mount_fs(Some(efi_part.as_str()), &efi_mount, Some("vfat"), MsFlags::empty())?;

    let efi_boot = format!("{}/EFI/BOOT", efi_mount);
    let _ = fs::create_dir_all(&efi_boot);
    
    fs::copy("/boot/efi/EFI/BOOT/BOOTX64.EFI", format!("{}/BOOTX64.EFI", efi_boot))?;
    fs::copy("/boot/vmlinuz", format!("{}/vmlinuz", efi_mount))?;
    fs::copy("/boot/initrd.img", format!("{}/initrd.img", efi_mount))?;

    let grub_cfg = format!(
        "set timeout=5\nset default=0\n\nmenuentry \"SkoodaOS\" {{\n    linux /vmlinuz root={} rw quiet\n    initrd /initrd.img\n}}\n",
        root_part
    );
    fs::write(format!("{}/grub.cfg", efi_boot), grub_cfg)?;

    let _ = mounter.umount_fs(&efi_mount);
    let _ = mounter.umount_fs(target_mount);

    info!("SUCCESS: SkoodaOS installed to {}. You can now reboot.", target);
    Ok(())
}
