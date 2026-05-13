use std::fs;

fn main() {
    println!("{:<12} {:<10} {:<10}", "PCI Address", "Vendor", "Device");
    println!("{:-<35}", "");

    if let Ok(entries) = fs::read_dir("/sys/bus/pci/devices") {
        for entry in entries.flatten() {
            let addr = entry.file_name().into_string().unwrap();
            let vendor = fs::read_to_string(entry.path().join("vendor")).unwrap_or_default().trim().to_string();
            let device = fs::read_to_string(entry.path().join("device")).unwrap_or_default().trim().to_string();
            
            println!("{:<12} {:<10} {:<10}", addr, vendor, device);
        }
    } else {
        println!("No PCI devices found. Is sysfs mounted?");
    }
}
