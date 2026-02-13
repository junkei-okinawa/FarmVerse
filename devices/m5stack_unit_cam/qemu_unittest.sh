#!/bin/sh

set -eu

TARGET="${TARGET:-xtensa-esp32-espidf}"
BUILD="${BUILD:-debug}"
BIN_NAME="${BIN_NAME:-sensor_data_sender}"
IMAGE_PATH="target/$TARGET/$BUILD/$BIN_NAME.bin"
ELF_PATH="target/$TARGET/$BUILD/$BIN_NAME"

find_qemu() {
    if [ -n "${ESP_QEMU_BIN:-}" ] && [ -x "${ESP_QEMU_BIN}" ]; then
        echo "${ESP_QEMU_BIN}"
        return
    fi

    if command -v qemu-system-xtensa >/dev/null 2>&1; then
        command -v qemu-system-xtensa
        return
    fi

    if [ -x "/usr/local/src/qemu/build/qemu-system-xtensa" ]; then
        echo "/usr/local/src/qemu/build/qemu-system-xtensa"
        return
    fi

    return 1
}

if ! command -v espflash >/dev/null 2>&1; then
    echo "Error: espflash is not installed."
    echo "Install with: cargo install espflash"
    exit 1
fi

QEMU_BIN="$(find_qemu || true)"
if [ -z "${QEMU_BIN}" ]; then
    echo "Error: qemu-system-xtensa was not found."
    echo "Set ESP_QEMU_BIN or install an ESP32-capable QEMU build."
    exit 1
fi

if [ "${QEMU_POC_SKIP_BUILD:-0}" != "1" ]; then
    if [ -z "${IDF_PATH:-}" ]; then
        echo "Error: IDF_PATH is not set. Activate ESP-IDF first."
        echo "Example: . /Users/junkei/esp/v5.1.6/esp-idf/export.sh"
        exit 1
    fi

    echo "[qemu-poc] Building firmware..."
    VIRTUAL_ENV="" cargo +esp build --target "${TARGET}"
fi

if [ ! -f "${ELF_PATH}" ]; then
    echo "Error: ELF not found at ${ELF_PATH}"
    echo "Build likely failed before image generation."
    exit 1
fi

echo "[qemu-poc] Creating flash image..."
espflash save-image --chip esp32 --merge "${ELF_PATH}" "${IMAGE_PATH}"

if command -v timeout >/dev/null 2>&1; then
    TIMEOUT_CMD="timeout 15"
elif command -v gtimeout >/dev/null 2>&1; then
    TIMEOUT_CMD="gtimeout 15"
else
    TIMEOUT_CMD=""
fi

echo "[qemu-poc] Starting QEMU: ${QEMU_BIN}"
if [ -n "${TIMEOUT_CMD}" ]; then
    # shellcheck disable=SC2086
    ${TIMEOUT_CMD} "${QEMU_BIN}" -nographic -machine esp32 -drive "file=${IMAGE_PATH},if=mtd,format=raw" -no-reboot
else
    "${QEMU_BIN}" -nographic -machine esp32 -drive "file=${IMAGE_PATH},if=mtd,format=raw" -no-reboot
fi
