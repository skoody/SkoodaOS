# SkoodaOS Build & Workflow Automation

TARGET := "x86_64-unknown-linux-musl"

# Build all components in release mode
build:
    cargo build --release --target {{TARGET}}

# Build the rootfs and generate the ISO
iso: build
    ./scripts/make_iso.sh

# Run SkoodaOS in QEMU
qemu: iso
    qemu-system-x86_64 \
      -m 1024 \
      -smp 2 \
      -enable-kvm \
      -cdrom skoodaos.iso \
      -serial stdio \
      -nographic \
      -display none \
      -boot d

# Fast check for the target architecture
check:
    cargo check --target {{TARGET}}

# Strict linting
clippy:
    cargo clippy --target {{TARGET}} -- -D warnings

# Automatic fixes
fix:
    cargo fix --target {{TARGET}} --allow-dirty --allow-staged

# Clean build artifacts
clean:
    cargo clean
    rm -rf build_rootfs iso_staging skoodaos.iso skoodaos_initrd.img

# Run all tests
test:
    cargo test --workspace
