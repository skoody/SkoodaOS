#!/bin/bash
set -e

PROJECT_ROOT=$(pwd)
BUILD_DIR="$PROJECT_ROOT/build_rootfs"
TARGET="x86_64-unknown-linux-musl"

echo "--- Building SkoodaOS RootFS (DYNAMIC HARDWARE EDITION) ---"

# 1. Rust Komponenten bauen
cargo build --release --target $TARGET

# 2. RootFS Struktur erstellen
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"/{bin,etc,dev,proc,sys,run,lib,var,tmp,sbin}

# 3. Binaries kopieren
cp "$PROJECT_ROOT/target/$TARGET/release/skooda-init" "$BUILD_DIR/bin/init"
cp "$PROJECT_ROOT/target/$TARGET/release/skooda-sh" "$BUILD_DIR/bin/skooda-sh"
cp "$PROJECT_ROOT/target/$TARGET/release/skooda-install" "$BUILD_DIR/bin/skooda-install"
cp "$PROJECT_ROOT/target/$TARGET/release/skooda-update" "$BUILD_DIR/bin/skooda-update"
cp "$PROJECT_ROOT/target/$TARGET/release/cat" "$BUILD_DIR/bin/cat"
cp "$PROJECT_ROOT/target/$TARGET/release/mkdir" "$BUILD_DIR/bin/mkdir"
cp "$PROJECT_ROOT/target/$TARGET/release/rm" "$BUILD_DIR/bin/rm"
cp "$PROJECT_ROOT/target/$TARGET/release/reboot" "$BUILD_DIR/bin/reboot"
cp "$PROJECT_ROOT/target/$TARGET/release/poweroff" "$BUILD_DIR/bin/poweroff"
cp "$PROJECT_ROOT/target/$TARGET/release/skooda-net" "$BUILD_DIR/bin/skooda-net"

ln -sf /bin/init "$BUILD_DIR/sbin/init"
ln -sf /bin/init "$BUILD_DIR/init"
touch "$BUILD_DIR/etc/skoodaos-initramfs"

# 4. Kernel Module Vorbereitung (DYNAMIC)
KVER=$(uname -r)
MOD_DEST_BASE="$BUILD_DIR/lib/modules/$KVER"
mkdir -p "$MOD_DEST_BASE"

echo "[2/3] Copying kernel module structure for auto-loading..."
# Wir kopieren die gesamte Struktur, damit modprobe ALLES finden kann
cp -r "/lib/modules/$KVER/kernel" "$MOD_DEST_BASE/"
cp "/lib/modules/$KVER/modules."* "$MOD_DEST_BASE/" 2>/dev/null || true

# 5. Installer-Tools & Libraries kopieren (Alpine musl binaries)
echo "[3/4] Fetching generic x86_64 installer tools from Alpine via Docker..."
ALPINE_DIR="$PROJECT_ROOT/alpine_tools"
rm -rf "$ALPINE_DIR"
mkdir -p "$ALPINE_DIR"

docker run --rm -v "$ALPINE_DIR:/out" alpine sh -c "\
    apk add --no-cache util-linux e2fsprogs dosfstools coreutils wpa_supplicant busybox && \
    cp /sbin/sfdisk /sbin/mkfs.ext4 /sbin/mkfs.vfat /sbin/mkfs.fat /out/ && \
    cp /bin/mount /bin/umount /bin/cp /out/ && \
    cp /sbin/wpa_supplicant /sbin/wpa_cli /out/ && \
    cp /bin/busybox /out/ && \
    mkdir -p /out/lib /out/usr/lib && \
    cp -d /lib/*.so* /out/lib/ 2>/dev/null || true && \
    cp -d /usr/lib/*.so* /out/usr/lib/ 2>/dev/null || true && \
    chown -R $(id -u):$(id -g) /out"

cp "$ALPINE_DIR"/sfdisk "$BUILD_DIR/bin/"
cp "$ALPINE_DIR"/mkfs.* "$BUILD_DIR/bin/"
cp "$ALPINE_DIR"/mount "$BUILD_DIR/bin/"
cp "$ALPINE_DIR"/umount "$BUILD_DIR/bin/"
cp "$ALPINE_DIR"/cp "$BUILD_DIR/bin/"
cp "$ALPINE_DIR"/wpa_supplicant "$BUILD_DIR/bin/"
cp "$ALPINE_DIR"/wpa_cli "$BUILD_DIR/bin/"
cp "$ALPINE_DIR"/busybox "$BUILD_DIR/bin/"

ln -sf /bin/busybox "$BUILD_DIR/bin/mdev"
ln -sf /bin/busybox "$BUILD_DIR/bin/modprobe"
ln -sf /bin/busybox "$BUILD_DIR/bin/lsmod"
ln -sf /bin/busybox "$BUILD_DIR/bin/depmod"

cp -rd "$ALPINE_DIR"/lib/* "$BUILD_DIR/lib/"
mkdir -p "$BUILD_DIR/usr/lib"
cp -rd "$ALPINE_DIR"/usr/lib/* "$BUILD_DIR/usr/lib/" 2>/dev/null || true
rm -rf "$ALPINE_DIR"

# 6. Standalone GRUB Payload generieren
mkdir -p "$BUILD_DIR/boot/efi/EFI/BOOT"
grub-mkimage -O x86_64-efi -o "$BUILD_DIR/boot/efi/EFI/BOOT/BOOTX64.EFI" -p /EFI/BOOT fat ext2 part_gpt normal boot linux configfile loopback chain search || true

# 7. Default Services Config
echo "[5/6] Creating default services.toml..."
cat <<EOF > "$BUILD_DIR/etc/services.toml"
[[services]]
name = "network"
command = "/bin/skooda-net"
args = ["daemon"]
restart_policy = "OnFailure"

[[services]]
name = "shell"
command = "/bin/skooda-sh"
restart_policy = "Always"
EOF

# 8. Firmware Optimization (DYNAMIC)
echo "[6/6] Copying essential firmware for auto-detection..."
FW_BASE="$BUILD_DIR/lib/firmware"
mkdir -p "$FW_BASE"

# Nur die wichtigsten Treiber-Familien, den Rest soll modprobe/mdev machen
cp -r /lib/firmware/rtl_nic "$FW_BASE/" 2>/dev/null || true
cp -r /lib/firmware/rtw8* "$FW_BASE/" 2>/dev/null || true
cp -r /lib/firmware/iwlwifi* "$FW_BASE/" 2>/dev/null || true
cp -r /lib/firmware/amdgpu "$FW_BASE/" 2>/dev/null || true

echo "--- RootFS ready! ---"
