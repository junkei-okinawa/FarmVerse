#!/bin/sh

# You might need to change this...
ESP_QEMU_PATH=/usr/local/src/qemu/build
BUILD=debug
TARGET=xtensa-esp32-espidf # Don't change this. Only the ESP32 chip is supported in QEMU for now
PACKAGE_NAME=$(cargo get package.name) # requires `cargo-get` crate
if [ -z "$PACKAGE_NAME" ]; then
    echo "Error: Could not determine package name. Make sure you have the 'cargo-get' crate installed."
    exit 1
fi

output_path=$(VIRTUAL_ENV="" cargo test --no-run 2>&1 | grep -oP '(?<=Executable unittests src/main.rs \().*?(?=\))')
espflash save-image --chip esp32 --merge $output_path target/$TARGET/$BUILD/$PACKAGE_NAME.bin
$ESP_QEMU_PATH/qemu-system-xtensa -nographic -machine esp32 -drive file=target/$TARGET/$BUILD/$PACKAGE_NAME.bin,if=mtd,format=raw -no-reboot