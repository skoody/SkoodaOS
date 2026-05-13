# SkoodaOS

A minimalist, high-performance, and secure Linux-based operating system built from scratch in Rust.

## Features

- **PID 1 Service Supervisor:** An async init system (`skooda-init`) that manages system services with dependency tracking and auto-restart.
- **Async Networking Daemon:** `skooda-net` handles Ethernet (DHCP) and WiFi (wpa_supplicant) asynchronously with interface prioritization.
- **Secure A/B Updates:** Cryptographically signed atomic updates using Ed25519 signatures and SHA256 checksums.
- **Image-based Installer:** Fast installation process using Zstd-compressed rootfs images.
- **Custom Shell:** `skooda-sh` with support for Unix pipes and IO redirection.
- **Shared Utils Library:** Centralized core logic in `skooda-utils` for consistent dmesg, mounting, and logging.

## Architecture

SkoodaOS is built with a focus on:
1. **Safety:** Leveraging Rust's memory safety for core system components.
2. **Performance:** Minimalist design with low-level optimizations (ioctl, netlink).
3. **Robustness:** Async-first approach to avoid blocking system tasks.

## Getting Started

### Prerequisites

- Rust (latest stable)
- `musl` target: `rustup target add x86_64-unknown-linux-musl`
- Docker (for building installer tools)
- `grub-mkrescue`, `xorriso` (for ISO generation)

### Building the ISO

To build the bootable ISO image:

```bash
./scripts/make_iso.sh
```

The resulting `skoodaos.iso` can be written to a USB stick or booted in QEMU.

## License

This project is licensed under the MIT License.
