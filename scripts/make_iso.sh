#!/bin/bash
set -e

PROJECT_ROOT=$(pwd)
BUILD_DIR="$PROJECT_ROOT/build_rootfs"
ISO_DIR="$PROJECT_ROOT/iso_staging"
ISO_OUTPUT="$PROJECT_ROOT/skoodaos.iso"
KERNEL="/boot/vmlinuz-linux-cachyos"

echo "--- Generating SkoodaOS ISO for USB/Ventoy ---"

# 1. RootFS Vorbereitung
./scripts/build_rootfs.sh

# 2. Kernel ins RootFS kopieren
echo "[1/6] Embedding kernel into RootFS..."
mkdir -p "$BUILD_DIR/boot"
cp "$KERNEL" "$BUILD_DIR/boot/vmlinuz"

# 3. Kleines Initrd fuer das installierte System
echo "[2/6] Creating small initrd for the installed system..."
cd "$BUILD_DIR"
find . -maxdepth 1 ! -name "rootfs.tar.zst" ! -name "." | xargs find | cpio -o -H newc | zstd -3 -T0 > "$PROJECT_ROOT/small_initrd.img"
cd "$PROJECT_ROOT"
cp "$PROJECT_ROOT/small_initrd.img" "$BUILD_DIR/boot/initrd.img"

# 4. RootFS Image fuer den Installer erstellen
echo "[3/6] Generating rootfs.tar.zst for the installer..."
cd "$BUILD_DIR"
# Wir packen alles außer eventuelle alte Images
tar --exclude="rootfs.tar.zst" --exclude="live_initrd.img" -cf - . | zstd -3 -T0 > "$PROJECT_ROOT/rootfs.tar.zst"
cd "$PROJECT_ROOT"

# 5. Finales riesiges Initrd fuer die Live-ISO
echo "[4/6] Creating final live initrd (huge)..."
# Wir kopieren das rootfs.tar.zst KURZZEITIG in das build_rootfs, damit cpio es findet
cp "$PROJECT_ROOT/rootfs.tar.zst" "$BUILD_DIR/rootfs.tar.zst"
cd "$BUILD_DIR"
find . | cpio -o -H newc | zstd -3 -T0 > "$PROJECT_ROOT/live_initrd.img"
cd "$PROJECT_ROOT"
# SOFORT wieder löschen, damit es nicht im RootFS permanent Platz wegnimmt
rm -f "$BUILD_DIR/rootfs.tar.zst"

# 6. ISO Staging
echo "[5/6] Staging ISO files..."
rm -rf "$ISO_DIR"
mkdir -p "$ISO_DIR/boot/grub"

cp "$PROJECT_ROOT/live_initrd.img" "$ISO_DIR/boot/initrd.img"
cp "$KERNEL" "$ISO_DIR/boot/vmlinuz"

cat <<EOF > "$ISO_DIR/boot/grub/grub.cfg"
set timeout=2
set default=0

menuentry "SkoodaOS Live (Install Mode)" {
    linux /boot/vmlinuz quiet loglevel=3 splash
    initrd /boot/initrd.img
}
EOF

echo "[6/6] Building ISO: $ISO_OUTPUT..."
grub-mkrescue -o "$ISO_OUTPUT" "$ISO_DIR"

# Final Cleanup
echo "Cleaning up temporary images..."
rm -f "$PROJECT_ROOT/small_initrd.img" "$PROJECT_ROOT/live_initrd.img" "$PROJECT_ROOT/rootfs.tar.zst"

echo "DONE! Copy $ISO_OUTPUT to your USB stick."
