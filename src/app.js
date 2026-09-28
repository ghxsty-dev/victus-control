const tauriCore = (window.__TAURI__ && window.__TAURI__.core) ? window.__TAURI__.core : null;
const invoke = tauriCore ? tauriCore.invoke.bind(tauriCore) : null;
const logEl = document.getElementById('log');
function log(msg) {
  const t = new Date().toLocaleTimeString();
  logEl.textContent = `[${t}] ${msg}\n` + logEl.textContent;
}

let curve = [
  { temp: 45, speed: 20 },
  { temp: 60, speed: 40 },
  { temp: 70, speed: 65 },
  { temp: 80, speed: 85 },
  { temp: 90, speed: 100 },
];

function needTauri() {
  if (!invoke) {
    const banner = document.getElementById('backend-banner');
    banner.classList.remove('hidden');
    banner.textContent = 'Tauri arka ucu yok: bu sayfa tarayıcıda açılmış. Veriler için uygulamayı çalıştırın (src-tauri içinde cargo run --release) ve açılan Victus Control penceresini kullanın.';
    return false;
  }
  return true;
}

async function refreshSensors() {
  if (!invoke) return;
  try {
    const s = await invoke('get_sensors');
    const f = (v) => (v == null ? '—' : `${v.toFixed(1)}°C`);
    document.getElementById('t-cpu').textContent = f(s.cpu_temp);
    document.getElementById('t-apu').textContent = f(s.apu_temp);
    document.getElementById('t-nvidia').textContent = f(s.nvidia_temp);
    document.getElementById('t-extra').textContent =
      `${s.nvme_temp != null ? s.nvme_temp.toFixed(0) + '°C' : '—'} / ${s.acpi_temp != null ? s.acpi_temp.toFixed(0) + '°C' : '—'}`;
    document.getElementById('t-max').textContent = f(s.max_temp);
    document.getElementById('t-gpupow').textContent = s.nvidia_power != null ? `${s.nvidia_power.toFixed(1)}W (lim ${s.nvidia_power_limit ?? '?' }W)` : '—';
    document.getElementById('t-cpupow').textContent = s.cpu_power_avg != null ? `${s.cpu_power_avg.toFixed(1)}W` : '—';
    const rpm = (v) => (v == null ? '—' : `${v} RPM`);
    document.getElementById('fan-rpm').textContent = `${rpm(s.fan1_rpm)} / ${rpm(s.fan2_rpm)}`;
  } catch (e) { log('Sensör hatası: ' + e); }
}

async function refreshFan() {
  if (!invoke) return;
  try {
    const st = await invoke('get_fan_status');
    document.getElementById('fan-backend').textContent = st.backend;
    document.getElementById('fan-detail').textContent = st.detail;
    document.getElementById('fan-speed').textContent = `%${st.current_speed}`;
    document.getElementById('fan-temp').textContent = st.current_temp != null ? `${st.current_temp.toFixed(1)}°C` : '—';
    document.getElementById('auto-fan').checked = st.auto_fan;
    const banner = document.getElementById('backend-banner');
    if (st.backend === 'unsupported') {
      banner.classList.remove('hidden');
      banner.textContent = 'Fan yazma desteklenmiyor: ' + st.detail;
    } else { banner.classList.add('hidden'); }
  } catch (e) { log('Fan durum hatası: ' + e); }
}

async function refreshGpuLimits() {
  if (!invoke) return;
  try {
    const l = await invoke('get_gpu_limits');
    document.getElementById('gpu-min').textContent = l.min;
    document.getElementById('gpu-max').textContent = l.max;
    document.getElementById('gpu-def').textContent = l.default;
    document.getElementById('gpu-cur').textContent = (l.current ?? '?') + 'W';
    const slider = document.getElementById('in-gpu');
    slider.min = Math.ceil(l.min); slider.max = Math.floor(l.max);
    if (l.current) { slider.value = Math.round(l.current); document.getElementById('v-gpu').textContent = Math.round(l.current); }
  } catch (e) { log('GPU limit okunamadı: ' + e); }
}

function drawCurve() {
  const cv = document.getElementById('curve');
  const ctx = cv.getContext('2d');
  ctx.clearRect(0, 0, cv.width, cv.height);
  ctx.strokeStyle = '#2a3646';
  for (let t = 30; t <= 100; t += 10) {
    const x = ((t - 30) / 75) * cv.width;
    ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, cv.height); ctx.stroke();
  }
  const pts = [...curve].sort((a, b) => a.temp - b.temp);
  ctx.strokeStyle = '#2f7cf6'; ctx.lineWidth = 2; ctx.beginPath();
  pts.forEach((p, i) => {
    const x = ((p.temp - 30) / 75) * cv.width;
    const y = cv.height - (p.speed / 100) * cv.height;
    if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
  });
  ctx.stroke();
  ctx.fillStyle = '#fff';
  pts.forEach((p) => {
    const x = ((p.temp - 30) / 75) * cv.width;
    const y = cv.height - (p.speed / 100) * cv.height;
    ctx.beginPath(); ctx.arc(x, y, 5, 0, 7); ctx.fill();
  });
  ctx.lineWidth = 1;
}

function renderPoints() {
  const box = document.getElementById('curve-points');
  box.innerHTML = '';
  curve.forEach((p, i) => {
    const d = document.createElement('div');
    d.className = 'point';
    d.innerHTML = `<span>#${i + 1}</span>`;
    const t = document.createElement('input');
    t.type = 'number'; t.value = p.temp; t.min = 30; t.max = 105;
    t.onchange = () => { p.temp = Number(t.value); drawCurve(); };
    const s = document.createElement('input');
    s.type = 'number'; s.value = p.speed; s.min = 0; s.max = 100;
    s.onchange = () => { p.speed = Number(s.value); drawCurve(); };
    const del = document.createElement('button');
    del.textContent = 'x'; del.onclick = () => { curve.splice(i, 1); renderPoints(); drawCurve(); };
    d.append(t, document.createTextNode('°C'), s, document.createTextNode('%'), del);
    box.appendChild(d);
  });
  drawCurve();
}

async function showRaw() {
  if (!needTauri()) { log('Ham veri için Tauri penceresi gerekli.'); return; }
  try {
    const s = await invoke('get_sensors');
    log('HAM sensör: ' + JSON.stringify(s));
  } catch (e) { log('Ham veri hatası: ' + e); }
}

async function init() {
  renderPoints(); // grafik Tauri olmadan da çizilir
  if (!needTauri()) { log('Arayüz önizleme modunda (Tauri yok).'); return; }
  try {
    const tools = await invoke('get_cpu_tool_status');
    log('Araçlar: ' + JSON.stringify(tools));
    if (!tools.ryzenadj) log('Uyarı: ryzenadj yok — scripts/install-deps.sh çalıştırın.');
    if (!tools.nbfc) log('Bilgi: nbfc yok — fan backend unsupported olabilir.');
  } catch (e) { log('Araç kontrol hatası: ' + e); }
  try { curve = await invoke('get_fan_curve'); } catch {}
  renderPoints();
  await refreshSensors(); await refreshFan(); await refreshGpuLimits();
  setInterval(async () => {
    if (document.getElementById('auto-refresh').checked) { await refreshSensors(); await refreshFan(); }
  }, 2000);
}

document.getElementById('btn-refresh').onclick = async () => {
  if (!needTauri()) return;
  await refreshSensors(); await refreshFan(); await showRaw();
};
document.getElementById('btn-manual-fan').onclick = async () => {
  if (!needTauri()) return;
  const v = Number(document.getElementById('manual-fan').value);
  try { log(await invoke('set_manual_fan', { speed: v })); await refreshFan(); }
  catch (e) { log('Manuel fan hatası: ' + e); }
};
document.getElementById('auto-fan').onchange = async (e) => {
  if (!needTauri()) return;
  try { log(await invoke('set_auto_fan', { enabled: e.target.checked })); } catch (err) { log('Auto fan hatası: ' + err); }
};
document.getElementById('btn-add-point').onclick = () => { curve.push({ temp: 75, speed: 70 }); renderPoints(); };
document.getElementById('btn-reset-curve').onclick = () => {
  curve = [{ temp: 45, speed: 20 }, { temp: 60, speed: 40 }, { temp: 70, speed: 65 }, { temp: 80, speed: 85 }, { temp: 90, speed: 100 }];
  renderPoints();
};
document.getElementById('btn-save-curve').onclick = async () => {
  if (!needTauri()) return;
  try { log(await invoke('set_fan_curve', { curve })); } catch (e) { log('Eğri hatası: ' + e); }
};
for (const [id, vid] of [['in-stapm', 'v-stapm'], ['in-fast', 'v-fast'], ['in-slow', 'v-slow'], ['in-gpu', 'v-gpu']]) {
  document.getElementById(id).oninput = (e) => { document.getElementById(vid).textContent = e.target.value; };
}
document.getElementById('btn-cpu').onclick = async () => {
  if (!needTauri()) return;
  const req = {
    stapm_w: Number(document.getElementById('in-stapm').value),
    fast_w: Number(document.getElementById('in-fast').value),
    slow_w: Number(document.getElementById('in-slow').value),
  };
  try { log(await invoke('set_cpu_power', { req })); document.getElementById('cpu-status').textContent = 'Uygulandı.'; }
  catch (e) { log('CPU hatası: ' + e); }
};
document.getElementById('btn-gpu').onclick = async () => {
  if (!needTauri()) return;
  const v = Number(document.getElementById('in-gpu').value);
  try { log(await invoke('set_gpu_power', { limitW: v })); await refreshGpuLimits(); await refreshSensors(); }
  catch (e) { log('GPU hatası: ' + e); }
};

init();
