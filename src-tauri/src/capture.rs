//! Capture mode (ground truth): the user does one dictation with the button of the chatgpt.com page.
//! WebKit gives the app no DevTools Protocol, so a script at the start of each page wraps fetch and records the request
//! to /backend-api/transcribe. It sees the headers of the page, not the headers that WebKit adds (cookie, user-agent).
//! The capture file keeps names and shapes, and no secret value.
use crate::{app::moulin, chatgpt, report};
use serde_json::{Map, Value, json};
use std::time::Duration;
use tauri::AppHandle;

/// The code of the capture mode in the page (see capture.js).
const CAPTURE: &str = include_str!("capture.js");

/// Runs at the start of each page of the window, in capture mode only.
pub fn script() -> String {
    chatgpt::call(CAPTURE, "record", &[])
}

/// Headers with a value that is not a secret. The capture file redacts all other values.
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

/// The keys and the types of a JSON value, two levels deep. No values.
fn shape(value: &Value, depth: u8) -> Value {
    match value {
        Value::Array(items) if depth > 0 => json!([items.first().map_or(json!("empty"), |v| shape(v, depth - 1))]),
        Value::Array(_) => json!("array"),
        Value::Object(map) if depth > 0 => Value::Object(map.iter().map(|(k, v)| (k.clone(), shape(v, depth - 1))).collect()),
        // As in JavaScript: typeof null is "object".
        Value::Object(_) | Value::Null => json!("object"),
        Value::String(_) => json!("string"),
        Value::Number(_) => json!("number"),
        Value::Bool(_) => json!("boolean"),
    }
}

/// The parts of the form: name, file name, and content type. A short text value stays (for example a language code).
fn form(parts: &Value) -> Value {
    let parts = parts.as_array().cloned().unwrap_or_default();
    json!(
        parts
            .iter()
            .map(|part| {
                let mut out = json!({ "name": part["name"], "filename": part["filename"], "contentType": part["contentType"] });
                if let Some(value) = part["value"].as_str() {
                    out["value"] = json!(if looks_like_uuid(value) {
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

/// True if the text starts like a UUID: 8 and 4 hexadecimal digits, each with a "-" after it.
fn looks_like_uuid(text: &str) -> bool {
    let b = text.as_bytes();
    let hex = |range: std::ops::Range<usize>| b[range].iter().all(u8::is_ascii_hexdigit);
    b.len() >= 14 && hex(0..8) && b[8] == b'-' && hex(9..13) && b[13] == b'-'
}

/// Where the page can read each secret header value. The values stay in this function.
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
    let code = chatgpt::call(CAPTURE, "search", &[Value::Object(targets)]);
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
    let started_at = report::iso_now();
    let mut capture = json!({ "mode": "capture", "startedAt": started_at, "webview": tauri::webview_version().ok(), "requests": [] });
    let mut closes = chatgpt::closes(&app);
    chatgpt::show(&app);
    chatgpt::load(&app, chatgpt::HOME).await;
    println!("Capture: sign in, then do one dictation with the microphone button of the page. Do not send a message.");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15 * 60);
    let take = chatgpt::call(CAPTURE, "take", &[]);
    let reason = loop {
        tokio::select! {
            _ = closes.changed() => break "the user closed the window",
            _ = tokio::time::sleep_until(deadline) => break "timeout",
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
        let raws = chatgpt::evaluate(&app, &take, Duration::from_secs(10)).await.unwrap_or_default();
        let mut done = None;
        for raw in raws.as_array().cloned().unwrap_or_default() {
            let failed = raw["failed"].is_string();
            let entry = record(&app, &raw).await;
            if let Some(list) = capture["requests"].as_array_mut() {
                list.push(entry);
            }
            done = Some(if failed { "the request failed" } else { "captured" });
        }
        if let Some(reason) = done {
            break reason;
        }
    };
    capture["result"] = json!(reason);
    capture["fullPageMemoryMB"] = json!(chatgpt::memory_mb(&app).await);
    tokio::time::sleep(Duration::from_millis(1500)).await; // The page puts the text in the box after its request.
    capture["composerCleared"] = chatgpt::evaluate(&app, &chatgpt::call(CAPTURE, "clearComposer", &[]), Duration::from_secs(10)).await.unwrap_or_default();
    let file = out_dir.join(format!("capture-{}.json", report::stamp(&started_at)));
    match report::write(&file, &capture) {
        Ok(()) => println!("Capture: {reason}. File: {}", file.display()),
        Err(err) => eprintln!("Capture: {reason}. Cannot write {}: {err}", file.display()),
    }
    tokio::time::sleep(Duration::from_millis(1500)).await;
    app.exit(0);
}
