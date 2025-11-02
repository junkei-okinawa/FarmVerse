#!/bin/bash
# USB CDC Receiver Unit Tests Runner
# This script runs unit tests on the host machine without ESP32 hardware

set -e  # Exit on error

echo "🧪 Running USB CDC Receiver Unit Tests on Host Machine..."
echo ""

# Save original cargo config
if [ -f .cargo/config.toml ]; then
    echo "📦 Backing up .cargo/config.toml..."
    mv .cargo/config.toml .cargo/config.toml.backup
fi

# Detect host architecture
HOST_ARCH=$(rustc --version --verbose | grep host | awk '{print $2}')
echo "🖥️  Detected host architecture: $HOST_ARCH"

# Create host-specific cargo config
echo "⚙️  Creating host-specific Cargo configuration..."
mkdir -p .cargo
cat > .cargo/config.toml << EOF
[build]
target = "$HOST_ARCH"

[env]
# Disable ESP-IDF specific environment variables for host tests
EOF

# Function to cleanup on exit
cleanup() {
    echo ""
    echo "🧹 Cleaning up..."
    if [ -f .cargo/config.toml.backup ]; then
        mv .cargo/config.toml.backup .cargo/config.toml
        echo "✅ Restored .cargo/config.toml"
    fi
}

# Set trap to cleanup on exit
trap cleanup EXIT

echo ""
echo "🔨 Building and running tests..."
echo ""

# Run tests without ESP features
RUST_BACKTRACE=1 cargo test --lib --tests --no-default-features

echo ""
echo "✅ All tests passed!"
