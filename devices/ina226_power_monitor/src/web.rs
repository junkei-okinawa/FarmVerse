use std::sync::{Arc, Mutex};

use anyhow::Context;
use embedded_svc::{http::Method, io::Write as _};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::http::server::{Configuration as HttpConfiguration, EspHttpServer};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AuthMethod, BlockingWifi, Configuration as WifiConfiguration, EspWifi,
};
use log::info;

use crate::config::AppConfig;
use crate::model::Sample;

pub struct WebRuntime {
    _wifi: BlockingWifi<EspWifi<'static>>,
    _server: EspHttpServer<'static>,
}

pub fn start(
    modem: Modem,
    latest: Arc<Mutex<Sample>>,
    cfg: &AppConfig,
) -> anyhow::Result<WebRuntime> {
    let sys_loop = EspSystemEventLoop::take().context("failed to take system event loop")?;
    let nvs = EspDefaultNvsPartition::take().context("failed to take NVS partition")?;
    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sys_loop.clone(), Some(nvs))?, sys_loop)?;

    let mut ap_conf = AccessPointConfiguration::default();
    ap_conf.ssid = cfg
        .ap_ssid
        .as_str()
        .try_into()
        .map_err(|_| anyhow::anyhow!("AP SSID too long: {}", cfg.ap_ssid))?;
    ap_conf.channel = cfg.ap_channel;

    if cfg.ap_password.is_empty() {
        ap_conf.auth_method = AuthMethod::None;
        ap_conf.password.clear();
    } else {
        ap_conf.auth_method = AuthMethod::WPA2Personal;
        ap_conf.password = cfg
            .ap_password
            .as_str()
            .try_into()
            .map_err(|_| anyhow::anyhow!("AP password too long"))?;
    }

    let ssid_for_log = ap_conf.ssid.clone();
    wifi.set_configuration(&WifiConfiguration::AccessPoint(ap_conf))
        .context("failed to set Wi-Fi AP configuration")?;
    wifi.start().context("failed to start Wi-Fi AP")?;
    info!("Wi-Fi AP started: ssid={}", ssid_for_log);

    let mut server =
        EspHttpServer::new(&HttpConfiguration::default()).context("http init failed")?;

    let latest_for_index = Arc::clone(&latest);
    server.fn_handler("/", Method::Get, move |req| {
        let snapshot = latest_for_index
            .lock()
            .map_err(|_| anyhow::anyhow!("mutex poisoned"))?
            .clone();

        let html = build_index_html(&snapshot.target);
        let mut resp = req.into_response(
            200,
            Some("OK"),
            &[("Content-Type", "text/html; charset=utf-8")],
        )?;
        resp.write_all(html.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    let latest_for_metrics = Arc::clone(&latest);
    server.fn_handler("/metrics", Method::Get, move |req| {
        let snapshot = latest_for_metrics
            .lock()
            .map_err(|_| anyhow::anyhow!("mutex poisoned"))?
            .clone();
        let body = snapshot.to_json();

        let mut resp =
            req.into_response(200, Some("OK"), &[("Content-Type", "application/json")])?;
        resp.write_all(body.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    info!("HTTP endpoints: GET /, GET /metrics");

    Ok(WebRuntime {
        _wifi: wifi,
        _server: server,
    })
}

fn build_index_html(target: &str) -> String {
    format!(
        r##"<!doctype html>
<html lang="ja">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1"/>
  <title>INA226 Monitor</title>
  <style>
    body {{ font-family: sans-serif; margin: 16px; }}
    .row {{ display: flex; gap: 12px; flex-wrap: wrap; margin-bottom: 12px; }}
    .card {{ border: 1px solid #ccc; border-radius: 8px; padding: 10px; min-width: 160px; }}
    button {{ padding: 8px 12px; }}
    .tabs {{ display: flex; gap: 8px; margin: 12px 0; }}
    .tab-btn {{ padding: 8px 12px; border: 1px solid #ccc; background: #f7f7f7; border-radius: 6px; cursor: pointer; }}
    .tab-btn.active {{ background: #e6f0ff; border-color: #6fa3ff; }}
    .panel {{ display: none; }}
    .panel.active {{ display: block; }}
    .graph-wrap {{ border: 1px solid #ddd; border-radius: 8px; padding: 10px; background: #fff; }}
    canvas {{ width: 100%; height: 320px; display: block; }}
    .status {{ margin: 8px 0 12px; padding: 8px 10px; border-radius: 6px; font-size: 13px; border: 1px solid #ccc; background: #f7f7f7; }}
    .status.ok {{ border-color: #8fd19e; background: #eefaf1; color: #176b2f; }}
    .status.warn {{ border-color: #f0c36d; background: #fff9e8; color: #7b5a0a; }}
    .status.offline {{ border-color: #ee9a9a; background: #fff1f1; color: #8a1f1f; }}
    table {{ width: 100%; border-collapse: collapse; }}
    th, td {{ border-bottom: 1px solid #eee; padding: 6px; font-size: 12px; text-align: right; }}
    th:first-child, td:first-child {{ text-align: left; }}
  </style>
</head>
<body>
  <h2>INA226 Power Monitor</h2>
  <div>target: <b id="target">{target}</b></div>
  <div class="row">
    <div class="card"><div>Bus Voltage</div><b id="bus_v">-</b> V</div>
    <div class="card"><div>Current</div><b id="current_ma">-</b> mA</div>
    <div class="card"><div>Power</div><b id="power_mw">-</b> mW</div>
  </div>
  <div class="row">
    <button id="download">Download CSV</button>
    <button id="clear">Clear Stored Data</button>
    <span id="count"></span>
  </div>
  <div id="status_box" class="status">Initializing...</div>

  <div class="tabs">
    <button id="tab_table" class="tab-btn active">表</button>
    <button id="tab_graph" class="tab-btn">折れ線グラフ</button>
  </div>

  <section id="panel_table" class="panel active">
    <table>
      <thead>
        <tr><th>timestamp_ms</th><th>bus_v</th><th>current_ma</th><th>power_mw</th></tr>
      </thead>
      <tbody id="rows"></tbody>
    </table>
  </section>

  <section id="panel_graph" class="panel">
    <div class="row">
      <label>表示範囲
        <select id="window_select">
          <option value="30">30秒</option>
          <option value="60">1分</option>
          <option value="180">3分</option>
          <option value="300">5分</option>
          <option value="600">10分</option>
          <option value="all">全データ</option>
        </select>
      </label>
      <label>指標
        <select id="metric_select">
          <option value="current_ma">Current (mA)</option>
          <option value="bus_voltage_v">Bus Voltage (V)</option>
          <option value="power_mw">Power (mW)</option>
        </select>
      </label>
      <button id="apply_graph">切替</button>
    </div>
    <div class="graph-wrap">
      <canvas id="line_chart"></canvas>
      <div id="graph_caption" style="font-size:12px;color:#666;margin-top:8px;"></div>
    </div>
  </section>

<script>
const DB_NAME = "ina226_monitor_db";
const STORE = "samples";
const MAX_ROWS = 5000;
let db;
let currentTab = "table";
let selectedWindow = "30";
let selectedMetric = "current_ma";
const metricMeta = {{
  current_ma: {{ label: "Current (mA)", color: "#0077cc" }},
  bus_voltage_v: {{ label: "Bus Voltage (V)", color: "#00a35a" }},
  power_mw: {{ label: "Power (mW)", color: "#cc3300" }},
}};

function openDb() {{
  return new Promise((resolve, reject) => {{
    const req = indexedDB.open(DB_NAME, 1);
    req.onupgradeneeded = () => {{
      const d = req.result;
      if (!d.objectStoreNames.contains(STORE)) {{
        d.createObjectStore(STORE, {{ keyPath: "timestamp_ms" }});
      }}
    }};
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  }});
}}

function tx(store, mode="readonly") {{
  return db.transaction(store, mode).objectStore(store);
}}

async function putSample(s) {{
  await new Promise((resolve, reject) => {{
    const r = tx(STORE, "readwrite").put(s);
    r.onsuccess = () => resolve();
    r.onerror = () => reject(r.error);
  }});
}}

async function getAllSamples() {{
  return await new Promise((resolve, reject) => {{
    const r = tx(STORE).getAll();
    r.onsuccess = () => resolve(r.result || []);
    r.onerror = () => reject(r.error);
  }});
}}

async function trimOld() {{
  const all = await getAllSamples();
  if (all.length <= MAX_ROWS) return;
  all.sort((a,b) => a.timestamp_ms - b.timestamp_ms);
  const remove = all.length - MAX_ROWS;
  await new Promise((resolve, reject) => {{
    const store = tx(STORE, "readwrite");
    for (let i=0; i<remove; i++) store.delete(all[i].timestamp_ms);
    const done = store.transaction;
    done.oncomplete = () => resolve();
    done.onerror = () => reject(done.error);
  }});
}}

function renderLatest(s) {{
  document.getElementById("target").textContent = s.target;
  document.getElementById("bus_v").textContent = Number(s.bus_voltage_v).toFixed(3);
  document.getElementById("current_ma").textContent = Number(s.current_ma).toFixed(3);
  document.getElementById("power_mw").textContent = Number(s.power_mw).toFixed(3);
  const box = document.getElementById("status_box");
  const quality = (s.quality || "").toLowerCase();
  if (s.sensor_online === false || quality === "offline") {{
    box.className = "status offline";
    box.textContent = s.status_message || "Sensor Offline";
    return;
  }}
  if (quality === "invalid") {{
    box.className = "status warn";
    box.textContent = s.status_message || "Invalid sample (not stored)";
    return;
  }}
  box.className = "status ok";
  box.textContent = s.status_message || "OK";
}}

async function renderTable() {{
  const all = await getAllSamples();
  all.sort((a,b) => b.timestamp_ms - a.timestamp_ms);
  const head = all.slice(0, 30);
  const tbody = document.getElementById("rows");
  tbody.innerHTML = head.map(s =>
    `<tr><td>${{s.timestamp_ms}}</td><td>${{Number(s.bus_voltage_v).toFixed(3)}}</td><td>${{Number(s.current_ma).toFixed(3)}}</td><td>${{Number(s.power_mw).toFixed(3)}}</td></tr>`
  ).join("");
  document.getElementById("count").textContent = `stored samples: ${{all.length}}`;
}}

function resolveSpanMs(samples, windowValue) {{
  if (windowValue !== "all") {{
    return Math.max(1, Number(windowValue) * 1000);
  }}
  if (samples.length === 0) return 30 * 1000;

  const first = samples[0].timestamp_ms;
  const latest = samples[samples.length - 1].timestamp_ms;
  const duration = Math.max(1, latest - first);

  if (duration <= 30_000) return 30_000;
  if (duration <= 60_000) return 60_000;
  if (duration <= 180_000) return 180_000;
  if (duration <= 300_000) return 300_000;
  if (duration <= 600_000) return 600_000;
  return Math.ceil(duration / 600_000) * 600_000;
}}

function rangeLabelFromSpanMs(spanMs, windowValue) {{
  if (windowValue !== "all") return `${{Math.round(spanMs / 1000)}}秒(固定)`;
  if (spanMs === 30_000) return "30秒(自動)";
  if (spanMs === 60_000) return "1分(自動)";
  if (spanMs === 180_000) return "3分(自動)";
  if (spanMs === 300_000) return "5分(自動)";
  if (spanMs === 600_000) return "10分(自動)";
  return `${{Math.round(spanMs / 60000)}}分(自動・10分刻み)`;
}}

function filterByRange(samples, xStart, xEnd) {{
  return samples.filter(s => s.timestamp_ms >= xStart && s.timestamp_ms <= xEnd);
}}

function setTab(tab) {{
  currentTab = tab;
  const isTable = tab === "table";
  document.getElementById("tab_table").classList.toggle("active", isTable);
  document.getElementById("tab_graph").classList.toggle("active", !isTable);
  document.getElementById("panel_table").classList.toggle("active", isTable);
  document.getElementById("panel_graph").classList.toggle("active", !isTable);
}}

function drawLineChart(samples, metricKey, xStart, xEnd) {{
  const canvas = document.getElementById("line_chart");
  const rect = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.max(300, Math.floor(rect.width * dpr));
  canvas.height = Math.max(180, Math.floor(rect.height * dpr));
  const ctx = canvas.getContext("2d");
  ctx.scale(dpr, dpr);

  const width = rect.width;
  const height = rect.height;
  ctx.clearRect(0, 0, width, height);

  const m = {{ left: 48, right: 14, top: 12, bottom: 30 }};
  const plotW = width - m.left - m.right;
  const plotH = height - m.top - m.bottom;
  if (plotW <= 0 || plotH <= 0) {{
    return;
  }}
  if (samples.length < 1) {{
    ctx.fillStyle = "#666";
    ctx.font = "12px sans-serif";
    ctx.fillText("データ不足", m.left, m.top + 14);
    return;
  }}

  let minY = Number.POSITIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  for (const s of samples) {{
    const y = Number(s[metricKey]);
    if (Number.isFinite(y)) {{
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }}
  }}
  if (!Number.isFinite(minY) || !Number.isFinite(maxY)) {{
    minY = 0; maxY = 1;
  }}
  if (maxY - minY < 1e-9) {{
    const pad = Math.max(1, Math.abs(maxY) * 0.05);
    minY -= pad;
    maxY += pad;
  }}

  const xSpan = Math.max(1, xEnd - xStart);
  const ySpan = maxY - minY;

  const xToPx = (x) => m.left + ((x - xStart) / xSpan) * plotW;
  const yToPx = (y) => m.top + (1 - ((y - minY) / ySpan)) * plotH;

  ctx.strokeStyle = "#ddd";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(m.left, m.top);
  ctx.lineTo(m.left, m.top + plotH);
  ctx.lineTo(m.left + plotW, m.top + plotH);
  ctx.stroke();

  const meta = metricMeta[metricKey];
  ctx.strokeStyle = meta.color;
  ctx.lineWidth = 2;
  if (samples.length === 1) {{
    const x = xToPx(samples[0].timestamp_ms);
    const y = yToPx(Number(samples[0][metricKey]));
    ctx.beginPath();
    ctx.arc(x, y, 3, 0, Math.PI * 2);
    ctx.fillStyle = meta.color;
    ctx.fill();
  }} else {{
    ctx.beginPath();
    for (let i = 0; i < samples.length; i++) {{
      const x = xToPx(samples[i].timestamp_ms);
      const y = yToPx(Number(samples[i][metricKey]));
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }}
    ctx.stroke();
  }}

  ctx.fillStyle = "#666";
  ctx.font = "11px sans-serif";
  ctx.fillText(`${{maxY.toFixed(3)}}`, 4, m.top + 4);
  ctx.fillText(`${{minY.toFixed(3)}}`, 4, m.top + plotH);
  ctx.fillText("t", m.left + plotW + 4, m.top + plotH + 1);
  ctx.fillText("0s", m.left, m.top + plotH + 16);
  ctx.fillText(`${{Math.round(xSpan / 1000)}}s`, m.left + plotW - 28, m.top + plotH + 16);
}}

async function renderGraph() {{
  const all = await getAllSamples();
  all.sort((a,b) => a.timestamp_ms - b.timestamp_ms);
  const spanMs = resolveSpanMs(all, selectedWindow);
  const latest = all.length > 0 ? all[all.length - 1].timestamp_ms : spanMs;
  const xEnd = latest;
  const xStart = Math.max(0, xEnd - spanMs);
  const filtered = filterByRange(all, xStart, xEnd);
  drawLineChart(filtered, selectedMetric, xStart, xEnd);
  const label = metricMeta[selectedMetric].label;
  const rangeLabel = rangeLabelFromSpanMs(spanMs, selectedWindow);
  document.getElementById("graph_caption").textContent =
    `${{label}} | 範囲: ${{rangeLabel}} | points: ${{filtered.length}}`;
}}

async function poll() {{
  try {{
    const r = await fetch("/metrics", {{ cache: "no-store" }});
    if (!r.ok) return;
    const s = await r.json();
    renderLatest(s);
    if (s.sensor_online !== false && (s.quality || "ok") === "ok") {{
      await putSample(s);
      await trimOld();
    }}
    if (currentTab === "table") {{
      await renderTable();
    }} else {{
      await renderGraph();
    }}
  }} catch (e) {{
    console.log(e);
  }}
}}

function downloadCsv(rows) {{
  const header = "timestamp_ms,bus_raw,bus_voltage_v,current_raw,current_ma,power_raw,power_mw,target\\n";
  const body = rows.sort((a,b)=>a.timestamp_ms-b.timestamp_ms).map(s =>
    `${{s.timestamp_ms}},${{s.bus_raw}},${{s.bus_voltage_v}},${{s.current_raw}},${{s.current_ma}},${{s.power_raw}},${{s.power_mw}},${{s.target}}`
  ).join("\\n");
  const blob = new Blob([header + body + "\\n"], {{ type: "text/csv" }});
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = "ina226_samples.csv";
  a.click();
  URL.revokeObjectURL(a.href);
}}

document.getElementById("download").addEventListener("click", async () => {{
  const rows = await getAllSamples();
  downloadCsv(rows);
}});

document.getElementById("clear").addEventListener("click", async () => {{
  await new Promise((resolve, reject) => {{
    const r = tx(STORE, "readwrite").clear();
    r.onsuccess = () => resolve();
    r.onerror = () => reject(r.error);
  }});
  await renderTable();
}});

document.getElementById("tab_table").addEventListener("click", async () => {{
  setTab("table");
  await renderTable();
}});

document.getElementById("tab_graph").addEventListener("click", async () => {{
  setTab("graph");
  await renderGraph();
}});

document.getElementById("apply_graph").addEventListener("click", async () => {{
  selectedWindow = document.getElementById("window_select").value;
  selectedMetric = document.getElementById("metric_select").value;
  await renderGraph();
}});

(async () => {{
  db = await openDb();
  await renderTable();
  await poll();
  setInterval(poll, 1000);
}})();
</script>
</body>
</html>"##
    )
}
