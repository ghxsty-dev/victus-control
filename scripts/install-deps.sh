#!/usr/bin/env bash
# Victus Control bağımlılık kurulumu (Ubuntu 24.04/26.04)
# Çalıştır: sudo ./scripts/install-deps.sh
set -e
apt update
apt install -y build-essential curl wget pkg-config \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
  libssl-dev lm-sensors i2c-tools git

echo "[1/3] Tauri build araçları kuruldu."

# ryzenadj (Ryzen 7840HS STAPM/PPT için)
if ! command -v ryzenadj >/dev/null 2>&1; then
  echo "[2/3] ryzenadj kuruluyor..."
  tmp=$(mktemp -d)
  git clone --depth 1 https://github.com/FlyGoat/RyzenAdj.git "$tmp/RyzenAdj"
  (cd "$tmp/RyzenAdj" && mkdir -p build && cd build && cmake -DCMAKE_BUILD_TYPE=Release .. && make -j"$(nproc)" && make install)
  rm -rf "$tmp"
else
  echo "[2/3] ryzenadj zaten kurulu."
fi

# nbfc-linux (fan kontrolü için, Victus EC profili gerekir)
if ! command -v nbfc >/dev/null 2>&1; then
  echo "[3/3] nbfc-linux kuruluyor..."
  if apt install -y nbfc 2>/dev/null; then
    echo "nbfc apt ile kuruldu."
  else
    echo "UYARI: apt içinde nbfc yok. Şu kaynaktan manuel kurun:"
    echo "  https://github.com/nbfc-linux/nbfc-linux"
    echo "Kurulum sonrası: sudo nbfc config -r  (önerilen Victus/OMEN profilini seçin)"
  fi
else
  echo "[3/3] nbfc zaten kurulu."
fi

sensors-detect --auto 2>/dev/null || true
echo "Tamam. Sonra: cargo build --release (victus-control/src-tauri içinde)"
