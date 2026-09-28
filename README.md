# Victus Control

HP Victus (Ryzen 7 7840HS + RTX 4060, Ubuntu) için **Tauri + Rust** masaüstü uygulaması:

- Sıcaklığa göre **fan eğrisi** (sıcaklık → %hız, 2 sn'de bir otomatik uygular)
- CPU **max güç limiti**: `ryzenadj` ile STAPM / PPT Fast / PPT Slow (8–80W)
- GPU **max güç limiti**: `nvidia-smi -pl` ile (kartın gördüğü min–max, örn. 5–120W)
- Canlı sıcaklık/güç izleme: k10temp, amdgpu, acpitz, nvme, nvidia-smi

Arayüz dili Türkçe. Ön yüz derlemesiz saf HTML/CSS/JS (`src/`), arka yüz Rust (`src-tauri/src/main.rs`). Node/npm gerekmez.

## Fan kontrolü gerçeği (16-s0xxx)

Stok Linux sürücüleri bu kasada **hiçbir fan düğümü açmıyor** (doğrulandı:
`pwm*`/`fan*_input` yok, `hp-wmi` hwmon yok, `platform_profile` yok).
`hp_omen_extra` modülü yalnız klavye RGB'sidir. Bu yüzden uygulama ilk
açılışta `unsupported` gösterir — bu bir hata değil, donanım gerçeğidir.

Gerçek fan kontrolü için yamalı `hp-wmi` DKMS modülü gerekir. Senin kasa
ailen (16-s00xx) için onaylı çözüm: **Batuhan4/victus-control**
(Gerçek eğri = Better Auto, Manuel RPM, MAX; Ubuntu destekli):

```bash
git clone https://github.com/Batuhan4/victus-control.git
cd victus-control
sudo ./install.sh
```

Secure Boot açıksa DKMS modülünü imzalaman (MOK) gerekebilir; kurulum
yönlendirmesini izle. Modül `pwm`/`fan` düğümlerini açınca bu uygulama
otomatik olarak `hwmon-pwm` arka ucuna geçer ve RPM göstermeye başlar.
Güç limitleri (ryzenadj + nvidia-smi) modülsüz de çalışır.

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
