// Victus Control - Tauri backend
// HP Victus (Ryzen 7 7840HS + RTX 4060) için:
// - Sıcaklık okuma: /sys/class/hwmon + nvidia-smi
// - CPU güç limiti: ryzenadj (STAPM / PPT fast / PPT slow)
// - GPU güç limiti: nvidia-smi -pl
// - Fan: nbfc-linux > hwmon pwm > unsupported (Victus EC genelde kilitlidir)

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager};

// ---------- Veri tipleri ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorData {
    pub cpu_temp: Option<f32>,
    pub apu_temp: Option<f32>,
    pub nvme_temp: Option<f32>,
    pub acpi_temp: Option<f32>,
    pub nvidia_temp: Option<f32>,
    pub nvidia_power: Option<f32>,
    pub nvidia_power_limit: Option<f32>,
    pub cpu_power_avg: Option<f32>,
    pub max_temp: Option<f32>,
    pub fan1_rpm: Option<u32>,
    pub fan2_rpm: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuLimits {
    pub current: Option<f32>,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanCurvePoint {
    pub temp: f32, // °C
    pub speed: u8, // %
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanStatus {
    pub backend: String, // "nbfc" | "hwmon-pwm" | "unsupported"
    pub detail: String,
    pub auto_fan: bool,
    pub current_speed: u8,
    pub current_temp: Option<f32>,
    pub pwm_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuPowerRequest {
    pub stapm_w: f32,
    pub fast_w: f32,
    pub slow_w: f32,
}

struct AppState {
    curve: Vec<FanCurvePoint>,
    auto_fan: bool,
    current_speed: u8,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            curve: vec![
                FanCurvePoint {
                    temp: 45.0,
                    speed: 20,
                },
                FanCurvePoint {
                    temp: 60.0,
                    speed: 40,
                },
                FanCurvePoint {
                    temp: 70.0,
                    speed: 65,
                },
                FanCurvePoint {
                    temp: 80.0,
                    speed: 85,
                },
                FanCurvePoint {
                    temp: 90.0,
                    speed: 100,
                },
            ],
            auto_fan: false,
            current_speed: 0,
        }
    }
}

// ---------- Yardımcılar ----------

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// hwmon sıcaklık girdisi miliderece -> santigrat
fn read_temp_c(path: &str) -> Option<f32> {
    let raw = read_trimmed(path)?;
    let milli: f64 = raw.parse().ok()?;
    Some((milli / 1000.0) as f32)
}

fn read_power_w(path: &str) -> Option<f32> {
    // hwmon power girdileri microwatt cinsindendir
    let raw = read_trimmed(path)?;
    let micro: f64 = raw.parse().ok()?;
    Some((micro / 1_000_000.0) as f32)
}

fn hwmon_name(hwmon: &str) -> String {
    read_trimmed(&format!("/sys/class/hwmon/{}/name", hwmon)).unwrap_or_default()
}

fn find_hwmon_by_name(target: &str) -> Option<String> {
    let entries = fs::read_dir("/sys/class/hwmon").ok()?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if hwmon_name(&name) == target {
            return Some(name);
        }
    }
    None
}

fn run_cmd(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{} çalıştırılamadı: {}", program, e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!(
            "{} hata: {}",
            program,
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Ayrıcalıklı komut: önce doğrudan dene, olmazsa pkexec ile dene.
fn run_privileged(program: &str, args: &[String]) -> Result<String, String> {
    let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match run_cmd(program, &str_args) {
        Ok(o) => Ok(o),
        Err(first_err) => {
            // pkexec fallback (GUI şifre penceresi açar)
            let mut full: Vec<&str> = vec![program];
            full.extend(str_args);
            match run_cmd("pkexec", &full) {
                Ok(o) => Ok(o),
                Err(_) => Err(format!(
                    "{}. pkexec ile de olmadı. Uygulamayı sudo/pkexec ile çalıştırın.",
                    first_err
                )),
            }
        }
    }
}

// ---------- Sensörler ----------

fn read_nvidia_csv() -> (Option<f32>, Option<f32>, Option<f32>) {
    // temp, power.draw, power.limit
    let out = run_cmd(
        "nvidia-smi",
        &[
            "--query-gpu=temperature.gpu,power.draw,power.limit",
            "--format=csv,noheader,nounits",
        ],
    );
    match out {
        Ok(line) => {
            let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
            if parts.len() >= 3 {
                let t = parts[0].parse::<f32>().ok();
                let p = parts[1].parse::<f32>().ok();
                let pl = parts[2].parse::<f32>().ok();
                return (t, p, pl);
            }
            (None, None, None)
        }
        Err(_) => (None, None, None),
    }
}

#[tauri::command]
fn get_sensors() -> SensorData {
    let mut s = SensorData {
        cpu_temp: None,
        apu_temp: None,
        nvme_temp: None,
        acpi_temp: None,
        nvidia_temp: None,
        nvidia_power: None,
        nvidia_power_limit: None,
        cpu_power_avg: None,
        max_temp: None,
    };

    if let Some(h) = find_hwmon_by_name("k10temp") {
        s.cpu_temp = read_temp_c(&format!("/sys/class/hwmon/{}/temp1_input", h));
    }
    if let Some(h) = find_hwmon_by_name("amdgpu") {
        s.apu_temp = read_temp_c(&format!("/sys/class/hwmon/{}/temp1_input", h));
        s.cpu_power_avg = read_power_w(&format!("/sys/class/hwmon/{}/power1_average", h))
            .or_else(|| read_power_w(&format!("/sys/class/hwmon/{}/power1_input", h)));
    }
    if let Some(h) = find_hwmon_by_name("nvme") {
        s.nvme_temp = read_temp_c(&format!("/sys/class/hwmon/{}/temp1_input", h));
    }
    if let Some(h) = find_hwmon_by_name("acpitz") {
        s.acpi_temp = read_temp_c(&format!("/sys/class/hwmon/{}/temp1_input", h));
    }

    let (nt, np, npl) = read_nvidia_csv();
    s.nvidia_temp = nt;
    s.nvidia_power = np;
    s.nvidia_power_limit = npl;

    // Fan RPM: yamalı hp-wmi (veya başka bir sürücü) fan*_input açarsa yakala.
    // Stok 16-s0xxx'te bu dosyalar yok; DKMS yaması kurulunca belirir.
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for e in entries.flatten() {
            let base = format!("/sys/class/hwmon/{}", e.file_name().to_string_lossy());
            if s.fan1_rpm.is_none() {
                if let Some(raw) = read_trimmed(&format!("{}/fan1_input", base)) {
                    s.fan1_rpm = raw.parse::<u32>().ok();
                }
            }
            if s.fan2_rpm.is_none() {
                if let Some(raw) = read_trimmed(&format!("{}/fan2_input", base)) {
                    s.fan2_rpm = raw.parse::<u32>().ok();
                }
            }
            if s.fan1_rpm.is_some() && s.fan2_rpm.is_some() {
                break;
            }
        }
    }

    let mut m: Option<f32> = None;
    for v in [s.cpu_temp, s.apu_temp, s.nvidia_temp, s.acpi_temp]
        .iter()
        .flatten()
    {
        m = Some(m.map_or(*v, |cur: f32| cur.max(*v)));
    }
    s.max_temp = m;
    s
}

// ---------- GPU güç limiti ----------

#[tauri::command]
fn get_gpu_limits() -> Result<GpuLimits, String> {
    let out = run_cmd(
        "nvidia-smi",
        &[
            "--query-gpu=power.limit,power.min_limit,power.max_limit,power.default_limit",
            "--format=csv,noheader,nounits",
        ],
    )?;
    let parts: Vec<&str> = out.split(',').map(|s| s.trim()).collect();
    if parts.len() < 4 {
        return Err(format!("nvidia-smi çıktısı anlaşılamadı: {}", out));
    }
    // Bazı sürücülerde power.limit CSV'de [N/A] döner; o zaman current boş geçilir.
    let parse_strict = |i: usize| -> Result<f32, String> {
        parts[i]
            .parse::<f32>()
            .map_err(|_| format!("güç değeri okunamadı: {}", parts[i]))
    };
    let current = parts[0].parse::<f32>().ok();
    Ok(GpuLimits {
        current,
        min: parse_strict(1)?,
        max: parse_strict(2)?,
        default: parse_strict(3)?,
    })
}

#[tauri::command]
fn set_gpu_power(limit_w: f32) -> Result<String, String> {
    if !(5.0..=200.0).contains(&limit_w) {
        return Err("GPU limiti 5-200W aralığında olmalı.".into());
    }
    let arg = format!("{}", limit_w as u32);
    run_privileged("nvidia-smi", &vec!["-pl".to_string(), arg])
        .map(|o| format!("GPU güç limiti {}W olarak ayarlandı. {}", limit_w as u32, o))
}

// ---------- CPU güç limiti (ryzenadj) ----------

fn ryzenadj_path() -> Option<String> {
    for p in ["/usr/bin/ryzenadj", "/usr/local/bin/ryzenadj"] {
        if Path::new(p).exists() {
            return Some(p.to_string());
        }
    }
    // PATH içinde ara
    run_cmd("which", &["ryzenadj"]).ok()
}

#[tauri::command]
fn get_cpu_tool_status() -> serde_json::Value {
    serde_json::json!({
        "ryzenadj": ryzenadj_path(),
        "nbfc": run_cmd("which", &["nbfc"]).ok(),
        "nvidia_smi": run_cmd("which", &["nvidia-smi"]).ok(),
    })
}

#[tauri::command]
fn set_cpu_power(req: CpuPowerRequest) -> Result<String, String> {
    for (label, v) in [
        ("STAPM", req.stapm_w),
        ("PPT Fast", req.fast_w),
        ("PPT Slow", req.slow_w),
    ] {
        if !(8.0..=80.0).contains(&v) {
            return Err(format!(
                "{} 8-80W aralığında olmalı (girilen: {})",
                label, v
            ));
        }
    }
    let bin = ryzenadj_path()
        .ok_or("ryzenadj bulunamadı. scripts/install-deps.sh ile kurun.".to_string())?;
    // ryzenadj limitleri mW bekler (örn. 45000 = 45W)
    let args = vec![
        format!("--stapm-limit={}", (req.stapm_w * 1000.0) as u32),
        format!("--fast-limit={}", (req.fast_w * 1000.0) as u32),
        format!("--slow-limit={}", (req.slow_w * 1000.0) as u32),
    ];
    run_privileged(&bin, &args).map(|o| {
        format!(
            "CPU limitleri STAPM:{}W Fast:{}W Slow:{}W. {}",
            req.stapm_w, req.fast_w, req.slow_w, o
        )
    })
}

// ---------- Fan kontrolü ----------

fn detect_pwm_paths() -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/hwmon") else {
        return out;
    };
    for e in entries.flatten() {
        let base = format!("/sys/class/hwmon/{}", e.file_name().to_string_lossy());
        for i in 1..=4 {
            let p = format!("{}/pwm{}", base, i);
            if Path::new(&p).exists() {
                out.push(p);
            }
        }
    }
    out
}

fn fan_backend() -> (String, String, Vec<String>) {
    if run_cmd("which", &["nbfc"]).is_ok() {
        return (
            "nbfc".into(),
            "nbfc-linux bulundu, EC üzerinden kontrol denenecek.".into(),
            vec![],
        );
    }
    let pwms = detect_pwm_paths();
    if !pwms.is_empty() {
        return (
            "hwmon-pwm".into(),
            format!("{} adet pwm girdisi bulundu.", pwms.len()),
            pwms,
        );
    }
    (
        "unsupported".into(),
        "Bu Victus 16-s0xxx'te stok sürücüler Linux'a fan girdisi açmıyor (EC kilitli, hp-wmi pwm yok). Gerçek fan kontrolü için yamalı hp-wmi DKMS modülü gerekir: Batuhan4/victus-control (16-s00xx onaylı, Ubuntu destekli) kurulumu yapın; modül pwm/fan düğümlerini açınca bu uygulama otomatik olarak hwmon-pwm arka ucuna geçer.".into(),
        vec![],
    )
}

fn interpolate_curve(curve: &[FanCurvePoint], temp: f32) -> u8 {
    if curve.is_empty() {
        return 0;
    }
    let mut sorted = curve.to_vec();
    sorted.sort_by(|a, b| a.temp.partial_cmp(&b.temp).unwrap());
    if temp <= sorted[0].temp {
        return sorted[0].speed;
    }
    if temp >= sorted[sorted.len() - 1].temp {
        return sorted[sorted.len() - 1].speed;
    }
    for w in sorted.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if temp >= a.temp && temp <= b.temp {
            let ratio = (temp - a.temp) / (b.temp - a.temp).max(0.001);
            let sp = a.speed as f32 + ratio * (b.speed as f32 - a.speed as f32);
            return sp.round().clamp(0.0, 100.0) as u8;
        }
    }
    0
}

fn apply_fan_percent(pct: u8) -> Result<String, String> {
    let pct = pct.min(100);
    let (backend, _, pwms) = fan_backend();
    match backend.as_str() {
        "nbfc" => {
            // Tüm fanlara uygula (0 ve 1). Hata verirse ilk hatayı döndür.
            let mut logs = Vec::new();
            for fan in ["0", "1"] {
                match run_privileged(
                    "nbfc",
                    &vec![
                        "set".into(),
                        "-f".into(),
                        fan.into(),
                        "-s".into(),
                        format!("{}", pct),
                    ],
                ) {
                    Ok(o) => logs.push(format!("fan{}: {}", fan, o)),
                    Err(e) => logs.push(format!("fan{} hata: {}", fan, e)),
                }
            }
            Ok(format!("nbfc ile %{} uygulandı. {}", pct, logs.join(" | ")))
        }
        "hwmon-pwm" => {
            let pwm_val = (pct as f32 / 100.0 * 255.0).round() as u32;
            let mut ok = 0;
            let mut errs = Vec::new();
            for p in &pwms {
                let enable = format!("{}_enable", p);
                // manuel moda almayı dene (hata olursa görmezden gel)
                let _ = run_privileged(
                    "sh",
                    &vec![
                        "-c".into(),
                        format!("echo 1 > {} 2>/dev/null; echo {} > {}", enable, pwm_val, p),
                    ],
                );
                // doğrudan yazma denemesi (root gerektirir)
                match (|| -> Result<(), String> {
                    let cmd = format!("echo {} > {}", pwm_val, p);
                    run_privileged("sh", &vec!["-c".into(), cmd])?;
                    Ok(())
                })() {
                    Ok(_) => ok += 1,
                    Err(e) => errs.push(e),
                }
            }
            if ok > 0 {
                Ok(format!(
                    "{} pwm girdisine %{} ({} /255) yazıldı.",
                    ok, pct, pwm_val
                ))
            } else {
                Err(format!("pwm yazılamadı: {}", errs.join(" | ")))
            }
        }
        _ => Err(
            "Fan yazma desteklenmiyor (unsupported). nbfc-linux kurun: scripts/install-deps.sh"
                .into(),
        ),
    }
}

#[tauri::command]
fn get_fan_status(state: tauri::State<Mutex<AppState>>) -> FanStatus {
    let (backend, detail, pwm_paths) = fan_backend();
    let st = state.lock().unwrap();
    let temp = get_sensors().max_temp;
    FanStatus {
        backend,
        detail,
        auto_fan: st.auto_fan,
        current_speed: st.current_speed,
        current_temp: temp,
        pwm_paths,
    }
}

#[tauri::command]
fn set_fan_curve(
    state: tauri::State<Mutex<AppState>>,
    curve: Vec<FanCurvePoint>,
) -> Result<String, String> {
    if curve.len() < 2 {
        return Err("En az 2 nokta gerekli.".into());
    }
    for p in &curve {
        if !(30.0..=105.0).contains(&p.temp) {
            return Err(format!("Sıcaklık 30-105°C olmalı (girilen {})", p.temp));
        }
        if p.speed > 100 {
            return Err("Hız %0-100 olmalı.".into());
        }
    }
    let mut st = state.lock().unwrap();
    let mut c = curve;
    c.sort_by(|a, b| a.temp.partial_cmp(&b.temp).unwrap());
    st.curve = c;
    Ok("Fan eğrisi kaydedildi.".into())
}

#[tauri::command]
fn get_fan_curve(state: tauri::State<Mutex<AppState>>) -> Vec<FanCurvePoint> {
    state.lock().unwrap().curve.clone()
}

#[tauri::command]
fn set_auto_fan(state: tauri::State<Mutex<AppState>>, enabled: bool) -> String {
    state.lock().unwrap().auto_fan = enabled;
    if enabled {
        "Otomatik fan eğrisi açıldı (2 sn'de bir uygulanır).".into()
    } else {
        "Otomatik fan kapatıldı.".into()
    }
}

#[tauri::command]
fn set_manual_fan(state: tauri::State<Mutex<AppState>>, speed: u8) -> Result<String, String> {
    if speed > 100 {
        return Err("Hız %0-100 olmalı.".into());
    }
    state.lock().unwrap().auto_fan = false;
    let msg = apply_fan_percent(speed)?;
    state.lock().unwrap().current_speed = speed;
    Ok(msg)
}

fn fan_daemon(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        let (enabled, curve) = {
            let Some(st) = app.try_state::<Mutex<AppState>>() else {
                continue;
            };
            let Ok(st) = st.try_lock() else {
                continue;
            };
            (st.auto_fan, st.curve.clone())
        };
        if !enabled {
            continue;
        }
        let temp = get_sensors().max_temp.unwrap_or(0.0);
        let target = interpolate_curve(&curve, temp);
        let _ = apply_fan_percent(target);
        if let Some(st) = app.try_state::<Mutex<AppState>>() {
            if let Ok(mut st) = st.try_lock() {
                st.current_speed = target;
            }
        }
        let _ = app.emit(
            "fan-tick",
            serde_json::json!({ "temp": temp, "speed": target }),
        );
    });
}

// ---------- main ----------

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(AppState::default()))
        .setup(|app| {
            fan_daemon(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_sensors,
            get_gpu_limits,
            set_gpu_power,
            get_cpu_tool_status,
            set_cpu_power,
            get_fan_status,
            get_fan_curve,
            set_fan_curve,
            set_auto_fan,
            set_manual_fan,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri çalıştırılamadı");
}
