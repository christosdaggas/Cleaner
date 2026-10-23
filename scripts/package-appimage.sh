#!/bin/bash
set -e

cd "$(dirname "$0")/.."

APP_NAME="Data Cleaner"
APP_ID="com.chrisdaggas.datacleaner"
VERSION="1.0.0"
BINARY_NAME="data-cleaner"
APPDIR="build/appimage/AppDir"
OUTPUT="dist/appimage/${APP_NAME// /_}-${VERSION}-$(uname -m).AppImage"

echo "=== Building AppImage Package ==="

echo "Building release binary..."
cargo build --release

LINUXDEPLOY="${LINUXDEPLOY:-linuxdeploy}"
APPIMAGETOOL="${APPIMAGETOOL:-appimagetool}"

if ! command -v "$LINUXDEPLOY" >/dev/null 2>&1; then
    echo "linuxdeploy is required to bundle GTK/libadwaita dependencies into the AppImage."
    exit 1
fi

if ! command -v linuxdeploy-plugin-gtk >/dev/null 2>&1; then
    echo "linuxdeploy-plugin-gtk is required to bundle GTK dependencies."
    exit 1
fi

if ! command -v "$APPIMAGETOOL" >/dev/null 2>&1; then
    echo "appimagetool is required to produce the final AppImage."
    exit 1
fi

# Create dist and build directories
mkdir -p dist/appimage
rm -rf build/appimage
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/metainfo"
mkdir -p "$APPDIR/usr/share/icons/hicolor/scalable/apps"
mkdir -p "$APPDIR/usr/share/icons/hicolor/symbolic/apps"

# Copy binary
cp "target/release/$BINARY_NAME" "$APPDIR/usr/bin/"

# Copy desktop file
cp "data/${APP_ID}.desktop" "$APPDIR/usr/share/applications/"

# Copy icons
cp "data/icons/hicolor/scalable/apps/${APP_ID}.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/"
cp "data/icons/hicolor/symbolic/apps/${APP_ID}-symbolic.svg" "$APPDIR/usr/share/icons/hicolor/symbolic/apps/"

# Copy metainfo
cp "data/${APP_ID}.metainfo.xml" "$APPDIR/usr/share/metainfo/"

# Create AppRun
cat > "$APPDIR/AppRun" << 'APPRUN'
#!/bin/bash
SELF=$(readlink -f "$0")
HERE=${SELF%/*}
export PATH="${HERE}/usr/bin:${PATH}"
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
exec "${HERE}/usr/bin/data-cleaner" "$@"
APPRUN
chmod +x "$APPDIR/AppRun"

# Create symlinks for AppImage
ln -sf "usr/share/applications/${APP_ID}.desktop" "$APPDIR/${APP_ID}.desktop"
ln -sf "usr/share/icons/hicolor/scalable/apps/${APP_ID}.svg" "$APPDIR/${APP_ID}.svg"
ln -sf "${APP_ID}.svg" "$APPDIR/.DirIcon"

echo "Bundling runtime libraries with linuxdeploy..."
"$LINUXDEPLOY" \
    --appdir "$APPDIR" \
    --desktop-file "data/${APP_ID}.desktop" \
    --icon-file "data/icons/hicolor/scalable/apps/${APP_ID}.svg" \
    --executable "target/release/${BINARY_NAME}" \
    --plugin gtk

# Build AppImage
ARCH="$(uname -m)" "$APPIMAGETOOL" --no-appstream "$APPDIR" "$OUTPUT"

echo ""
echo "✅ AppImage created!"
ls -lh "$OUTPUT"
