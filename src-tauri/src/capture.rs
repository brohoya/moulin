// Capture mode (ground truth): the user does one dictation with the button of the chatgpt.com page.
// WebKit gives the app no DevTools Protocol, so a script at the start of each page wraps fetch and records the request
// to /backend-api/transcribe. It sees the headers of the page, not the headers that WebKit adds (cookie, user-agent).
// The capture file keeps names and shapes, and no secret value.
use crate::{chatgpt, e2e::iso_now, moulin};
use serde_json::{Map, Value, json};
use std::time::Duration;
use tauri::AppHandle;

/// Runs at the start of each page of the window, in capture mode only.
pub const SCRIPT: &str = r#"(() => {
  if (location.origin !== 'https://chatgpt.com') return;
  const records = (window.__moulinCapture = []);
  const original = window.fetch;
  window.fetch = async function (input, init) {
    const url = String(typeof input === 'string' || input instanceof URL ? input : input.url);
    if (!/\/backend-api\/transcribe/.test(url)) return original.apply(this, arguments);
    const record = { method: (init && init.method) || (input && input.method) || 'GET', url: url.split('?')[0], headers: {}, form: [] };
    new Headers((init && init.headers) || (input instanceof Request ? input.headers : undefined)).forEach((value, name) => { record.headers[name] = value; });
    const body = init && init.body;
    if (body instanceof FormData) {
      for (const [name, value] of body.entries()) {
        record.form.push(value instanceof Blob ? { name, filename: value.name ?? null, contentType: value.type || null } : { name, value: String(value) });
      }
    }
    try {
      const response = await original.apply(this, arguments);
      const headers = {};
      response.headers.forEach((value, name) => { headers[name] = value; });
      record.response = { status: response.status, headers, body: await response.clone().text() };
      records.push(record);
      return response;
    } catch (e) {
      record.failed = String(e);
      records.push(record);
      throw e;
    }
  };
})();"#;

// Headers with a value that is not a secret. The capture file redacts all other values.
const SAFE: [&str; 21] = [
    "accept",
    "accept-encoding",
    "accept-language",
    "content-type",
    "content-length",
    "origin",
    "priority",
    "oai-language",
    "oai-client-version",
    "originator",
    "x-openai-web-frontend",
    "x-openai-codex-window-type",
    "sec-ch-ua",
    "sec-ch-ua-mobile",
    "sec-ch-ua-platform",
    "sec-fetch-dest",
    "sec-fetch-mode",
    "sec-fetch-site",
    "user-agent",
    "cf-mitigated",
    "server",
];

// The message box is the only contenteditable element of the page (October 2026).
const CLEAR_COMPOSER: &str = "(() => {
  const box = document.querySelector('[contenteditable=\"true\"]');
  if (!box || !box.textContent.trim()) return false;
  box.focus();
  document.execCommand('selectAll');
  document.execCommand('delete');
  return !box.textContent.trim();
})()";

// Runs in the page. Finds where the page can read each header value. Returns the places, not the values.
const PAGE_SEARCH: &str = "async (targets) => {
  const found = {};
  const visit = (value, where) => {
    if (typeof value === 'string') {
      for (const [name, target] of Object.entries(targets)) if (target && value === target) (found[name] ||= []).push(where);
      if (/^[[{]/.test(value)) { try { visit(JSON.parse(value), where); } catch {} }
    } else if (value && typeof value === 'object') {
      for (const [key, v] of Object.entries(value)) visit(v, `${where}.${key}`);
    }
  };
  let session = {};
  try { session = await (await fetch('/api/auth/session', { cache: 'no-store' })).json(); } catch {}
  visit(session, 'GET /api/auth/session');
  for (const store of ['localStorage', 'sessionStorage']) {
    for (let i = 0; i < window[store].length; i++) visit(window[store].getItem(window[store].key(i)), `${store}[${window[store].key(i)}]`);
  }
  for (const part of document.cookie.split('; ')) {
    const i = part.indexOf('=');
    try { visit(decodeURIComponent(part.slice(i + 1)), `document.cookie ${part.slice(0, i)}`); } catch {}
  }
  const shape = (v, depth = 2) => (Array.isArray(v) ? (depth ? [v.length ? shape(v[0], depth - 1) : 'empty'] : 'array')
    : v && typeof v === 'object' ? (depth ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, shape(x, depth - 1)])) : 'object') : typeof v);
  return { found, sessionShape: shape(session) };
}";

fn redact(headers: &Value) -> Value {
    let map = headers.as_object().cloned().unwrap_or_default();
    Value::Object(
        map.into_iter()
            .map(|(name, value)| {
                let key = name.to_lowercase();
                let text = value.as_str().unwrap_or_default();
                let kept = if SAFE.contains(&key.as_str()) { boundary(text) } else { "<redacted>".into() };
                (key, json!(kept))
            })
            .collect(),
    )
}

fn boundary(text: &str) -> String {
    match text.find("boundary=") {
        Some(at) => format!("{}boundary=<boundary>", &text[..at]),
        None => text.to_string(),
    }
}

// The keys and the types of a JSON value, two levels deep. No values.
fn shape(value: &Value, depth: u8) -> Value {
    match value {
        Value::Array(items) if depth > 0 => json!([items.first().map_or(json!("empty"), |v| shape(v, depth - 1))]),
        Value::Array(_) => json!("array"),
        Value::Object(map) if depth > 0 => Value::Object(map.iter().map(|(k, v)| (k.clone(), shape(v, depth - 1))).collect()),
        Value::Object(_) => json!("object"),
        Value::String(_) => json!("string"),
        Value::Number(_) => json!("number"),
        Value::Bool(_) => json!("boolean"),
        Value::Null => json!("object"),
    }
}

// The parts of the form: name, file name, and content type. A short text value stays (for example a language code).
fn form(parts: &Value) -> Value {
    let parts = parts.as_array().cloned().unwrap_or_default();
    json!(
        parts
            .iter()
            .map(|part| {
                let mut out = json!({ "name": part["name"], "filename": part["filename"], "contentType": part["contentType"] });
                if let Some(value) = part["value"].as_str() {
                    let b = value.as_bytes();
                    let hex = |range: std::ops::Range<usize>| b[range].iter().all(u8::is_ascii_hexdigit);
                    let uuid = b.len() >= 14 && hex(0..8) && b[8] == b'-' && hex(9..13) && b[13] == b'-';
                    out["value"] = json!(if uuid {
                        "<uuid>".into()
                    } else if value.chars().count() <= 40 {
                        value.to_string()
                    } else {
                        format!("<{} characters>", value.chars().count())
                    });
                }
                out
            })
            .collect::<Vec<_>>()
    )
}

// Where the page can read each secret header value. The values stay in this function.
async fn sources(app: &AppHandle, headers: &Value) -> Value {
    let mut targets = Map::new();
    for (name, value) in headers.as_object().cloned().unwrap_or_default() {
        let key = name.to_lowercase();
        let text = value.as_str().unwrap_or_default();
        if key == "authorization" {
            targets.insert(key, json!(text.trim_start_matches("Bearer ").trim_start_matches("bearer ")));
        } else if ["oai-", "openai-", "chatgpt-"].iter().any(|p| key.starts_with(p)) && !SAFE.contains(&key.as_str()) {
            targets.insert(key, json!(text));
        }
    }
    let mut found: Map<String, Value> = targets.keys().map(|k| (k.clone(), json!([]))).collect();
    for cookie in chatgpt::cookies(app) {
        for (key, target) in &targets {
            if target.as_str() == Some(cookie.value()) {
                let place = format!("cookie {}{}", cookie.name(), if cookie.http_only() == Some(true) { " (HttpOnly)" } else { "" });
                if let Some(list) = found.entry(key.clone()).or_insert_with(|| json!([])).as_array_mut() {
                    list.push(json!(place));
                }
            }
        }
    }
    let code = format!("({PAGE_SEARCH})({})", Value::Object(targets));
    let page = chatgpt::evaluate(app, &code, Duration::from_secs(30)).await.unwrap_or_default();
    for (key, places) in page["found"].as_object().cloned().unwrap_or_default() {
        let list = found.entry(key).or_insert_with(|| json!([]));
        if let (Some(list), Some(places)) = (list.as_array_mut(), places.as_array()) {
            list.extend(places.iter().cloned());
        }
    }
    json!({ "found": found, "sessionShape": page["sessionShape"] })
}

async fn record(app: &AppHandle, raw: &Value) -> Value {
    let mut out =
        json!({ "method": raw["method"], "url": raw["url"], "headersFromPage": redact(&raw["headers"]), "form": form(&raw["form"]) });
    if let Some(failed) = raw["failed"].as_str() {
        out["failed"] = json!(failed);
        return out;
    }
    let body = raw["response"]["body"].as_str().unwrap_or_default();
    let json: Option<Value> = serde_json::from_str(body).ok();
    out["response"] = json!({
        "status": raw["response"]["status"],
        "headers": redact(&raw["response"]["headers"]),
        "bodyShape": json.as_ref().map_or(json!(format!("<{} characters, not JSON>", body.len())), |j| shape(j, 2)),
    });
    // The transcript of the sample WAV is not a secret.
    if let Some(text) = json.as_ref().and_then(|j| j["text"].as_str()) {
        out["response"]["text"] = json!(text);
    }
    out["sources"] = sources(app, &raw["headers"]).await;
    out
}

pub async fn run(app: AppHandle) {
    let out_dir = moulin(&app).opts.out_dir.clone();
    let started_at = iso_now();
    let mut report = json!({ "mode": "capture", "startedAt": started_at, "webview": tauri::webview_version().ok(), "requests": [] });
    let mut closes = chatgpt::closes(&app);
    chatgpt::show(&app);
    chatgpt::load(&app, chatgpt::HOME).await;
    println!("Capture: sign in, then do one dictation with the microphone button of the page. Do not send a message.");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15 * 60);
    let reason = loop {
        tokio::select! {
            _ = closes.changed() => break "the user closed the window",
            _ = tokio::time::sleep_until(deadline) => break "timeout",
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
        let code = "window.__moulinCapture ? window.__moulinCapture.splice(0) : []";
        let raws = chatgpt::evaluate(&app, code, Duration::from_secs(10)).await.unwrap_or_default();
        let mut done = None;
        for raw in raws.as_array().cloned().unwrap_or_default() {
            let failed = raw["failed"].is_string();
            let entry = record(&app, &raw).await;
            if let Some(list) = report["requests"].as_array_mut() {
                list.push(entry);
            }
            done = Some(if failed { "the request failed" } else { "captured" });
        }
        if let Some(reason) = done {
            break reason;
        }
    };
    report["result"] = json!(reason);
    report["fullPageMemoryMB"] = json!(chatgpt::memory_mb(&app).await);
    tokio::time::sleep(Duration::from_millis(1500)).await; // The page puts the text in the box after its request.
    report["composerCleared"] = chatgpt::evaluate(&app, CLEAR_COMPOSER, Duration::from_secs(10)).await.unwrap_or_default();
    let _ = std::fs::create_dir_all(&out_dir);
    let file = out_dir.join(format!("capture-{}.json", started_at.replace([':', '.'], "-")));
    match serde_json::to_string_pretty(&report).map(|text| std::fs::write(&file, format!("{text}\n"))) {
        Ok(Ok(())) => println!("Capture: {reason}. File: {}", file.display()),
        _ => eprintln!("Capture: {reason}. Cannot write {}", file.display()),
    }
    tokio::time::sleep(Duration::from_millis(1500)).await;
    app.exit(0);
}
