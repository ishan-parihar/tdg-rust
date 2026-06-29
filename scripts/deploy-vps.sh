#!/usr/bin/env bash
# deploy-vps.sh — Extract binary from Docker, deploy to VPS
# Usage: ./scripts/deploy-vps.sh
set -euo pipefail

VPS_HOST="racknerd"
VPS_USER="root"
REMOTE_DIR="/usr/local/bin"
REMOTE_LIB="/usr/local/lib"

echo "==> Extracting binary and ORT libs from Docker build..."

# Find the latest build container
CONTAINER=$(docker ps -a --filter "status=exited" --format "{{.ID}}" | head -1)
if [ -z "$CONTAINER" ]; then
    echo "ERROR: No exited Docker container found. Run docker build first."
    exit 1
fi

TMPDIR=$(mktemp -d)
docker cp "$CONTAINER:/workspace/target/release/tdg-rust" "$TMPDIR/tdg-rust"
docker cp "$CONTAINER:/opt/ort/lib/." "$TMPDIR/ort-libs/"

echo "==> Binary info:"
file "$TMPDIR/tdg-rust"
ldd "$TMPDIR/tdg-rust" | head -5 || true

echo "==> Deploying to VPS ($VPS_HOST)..."
scp "$TMPDIR/tdg-rust" "$VPS_USER@$VPS_HOST:$REMOTE_DIR/tdg-rust"

# Copy ORT shared library
ssh "$VPS_USER@$VPS_HOST" "mkdir -p $REMOTE_LIB"
for f in "$TMPDIR/ort-libs"/libonnxruntime*; do
    scp "$f" "$VPS_USER@$VPS_HOST:$REMOTE_LIB/"
done

# Create ldconfig entry and run ldconfig
ssh "$VPS_USER@$VPS_HOST" "ldconfig"

# Verify
echo "==> Verifying on VPS..."
ssh "$VPS_USER@$VPS_HOST" "ldd $REMOTE_DIR/tdg-rust | grep ort"
ssh "$VPS_USER@$VPS_HOST" "$REMOTE_DIR/tdg-rust --version"

echo "==> Deploy complete!"
rm -rf "$TMPDIR"
