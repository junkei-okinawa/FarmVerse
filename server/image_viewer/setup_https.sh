#!/bin/bash

# 変数の設定
CERT_FILE="cert.pem"
KEY_FILE="key.pem"
PORT=8000
APP_NAME="app:app" # app.py の app インスタンスを指す

echo "--- 🔒 HTTPS Setup for FarmVerse Image Viewer ---"

# OpenSSLの存在確認
if ! command -v openssl &> /dev/null; then
    echo "❌ Error: openssl is not installed."
    echo "Please run: sudo apt install openssl"
    exit 1
fi

# 自己証明書の生成 (10年有効)
if [ ! -f "$CERT_FILE" ]; then
    echo "📄 Generating self-signed certificate..."
    openssl req -x509 -newkey rsa:4096 -keyout "$KEY_FILE" -out "$CERT_FILE" \
    -sha256 -days 3650 -nodes \
    -subj "/C=JP/ST=Tokyo/L=Shinjuku/O=FarmVerse/OU=Dev/CN=raspberrypi-base.local"
    echo "Certificate generated."
else
    echo "Certificate already exists. Skipping generation."
fi

# .env ファイルの確認
if [ ! -f ".env" ]; then
    echo "⚠️ .env file not found! Please create it with VIEWER_IMAGE_DIR."
    exit 1
fi
