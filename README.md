# Victus Control

HP Victus (Ryzen 7 7840HS + RTX 4060, Ubuntu) için **Tauri + Rust** masaüstü uygulaması:

- Sıcaklığa göre **fan eğrisi** (sıcaklık → %hız, 2 sn'de bir otomatik uygular)
- CPU **max güç limiti**: `ryzenadj` ile STAPM / PPT Fast / PPT Slow (8–80W)
- GPU **max güç limiti**: `nvidia-smi -pl` ile (kartın gördüğü min–max, örn. 5–120W)
- Canlı sıcaklık/güç izleme: k10temp, amdgpu, acpitz, nvme, nvidia-smi

Arayüz dili Türkçe. Ön yüz derlemesiz saf HTML/CSS/JS (`src/`), arka yüz Rust (`src-tauri/src/main.rs`). Node/npm gerekmez.

## Önemli donanım notu

Bu Victus'ta `/sys/class/hwmon` altında **fan/pwm girdisi yok** (EC kilitli — bu seride tipik).
Uygulama açılışta arka ucu otomatik seçer:

1. `nbfc` kuruluysa → `nbfc set -f 0/1 -s %` ile EC'ye yazar
2. `hwmon` pwm girdisi varsa → `/sys/.../pwm*` dosyasına yazar
3. Hiçbiri yoksa → `unsupported` gösterir; eğri yine hedef hızı hesaplar ama yazamaz

Kalıcı fan kontrolü için `scripts/install-deps.sh` ile **nbfc-linux** kurup uygun Victus/OMEN profilini seçin.

## Kurulum

```bash
sudo ./scripts/install-deps.sh
cd src-tauri
cargo build --release
./target/release/victus-control
```

Geliştirme modu:

```bash
cd src-tauri
cargo tauri dev
```

## Kullanım

1. Sıcaklıklar kartından değerleri izleyin.
2. Fan eğrisini düzenleyip **Eğriyi Kaydet** → **Otomatik fan** kutusunu işaretleyin.
3. CPU slider'ları (önerilen başlangıç: STAPM 35W / Fast 45W / Slow 40W) → Uygula (pkexec şifre sorar).
4. GPU slider (önerilen: 50–80W arası; varsayılan 80W) → Uygula.

## Güvenlik

Güç/fan yazma işlemleri root ister; uygulama `pkexec` ile şifre penceresi açar.
Değerleri kademeli değiştirin; 90°C+ sürekli yükte limitleri düşürün.

## Lisans

MIT
