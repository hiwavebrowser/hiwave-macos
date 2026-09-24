#!/usr/bin/env bash
#
# Cloud Agent bootstrap for HiWave / RustKit on Linux.
#
# HiWave is a macOS-first browser: the windowed application (hiwave-app,
# hiwave-smoke) and the macOS-font parity probes only build/run on macOS or
# Windows. What IS fully cross-platform is the RustKit browser engine and the
# headless `parity-capture` renderer, which parses HTML/CSS, lays out, and
# GPU-renders a frame with no display. This script prepares that toolchain.
#
# The script is idempotent: apt-get install is a no-op when packages are
# already present, rustup update converges on the latest stable, and cargo
# build reuses the existing target cache.
set -euo pipefail

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  SUDO="sudo"
fi

export DEBIAN_FRONTEND=noninteractive

# System dependencies:
#  - build-essential/pkg-config/cmake/libssl-dev: C toolchain for bundled
#    native deps (rusqlite, ring/rustls, wgpu shader tooling).
#  - libgtk-3-dev/libwebkit2gtk-4.1-dev/libsoup-3.0-dev: WRY/Tao chrome layer
#    used by hiwave-app on Linux.
#  - libasound2-dev: ALSA headers required by rustkit-media.
#  - mesa-vulkan-drivers/libvulkan1 + Mesa GL/EGL: software GPU (llvmpipe /
#    lavapipe) so wgpu can render headlessly without a physical GPU.
#  - fonts + fontconfig: baseline fonts for text shaping.
$SUDO apt-get update -qq
$SUDO apt-get install -y --no-install-recommends \
  build-essential pkg-config cmake libssl-dev \
  libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev \
  libasound2-dev \
  mesa-vulkan-drivers libvulkan1 \
  libgl1-mesa-dri libegl1 libegl1-mesa-dev libgles2-mesa-dev libglu1-mesa \
  fonts-dejavu-core fonts-liberation fontconfig

# Some transitive crates (e.g. the JS engine's `regress`) require Rust edition
# 2024, which needs Rust >= 1.85. The base image may ship an older stable, so
# converge on the latest stable toolchain.
rustup update stable
rustup default stable

# Warm the cache exactly like CI's build step. The headless renderer links the
# whole RustKit engine (DOM, CSS, layout, GPU compositor) and is the primary
# way to run/verify the engine on Linux with no display.
cargo build --release -p parity-capture
