//! The upload of a recording: the page sends it to /backend-api/transcribe, and Moulin reads the result.
use super::{CHALLENGE, HOME, Kind, evaluate, kind, load, on_chatgpt, start};
use crate::audio::Recording;
use base64::Engine;
use serde::Serialize;
use serde_json::{Value, json};
use std::time::Duration;
use tauri::AppHandle;

/// The time limit of the request in the page, in ms.
const TIMEOUT_MS: u64 = 120_000;
/// The error codes of ChatGPT with a message for the user.
const MESSAGES: [(&str, &str); 2] = [
    ("usage_limit_reached", "You reached the ChatGPT dictation limit. Try again later."),
    ("usage_not_included", "Your ChatGPT plan does not include dictation."),
];

/// The code for the page. It returns no secret: only the status, the text, and an error code.
/// The form fields and the headers are the same as the request of the dictation button (capture of October 2026).
/// The file is WAV instead of the WebM of the page: Moulin records the audio itself.
const UPLOAD: &str = r#"async ({ audio, sessionId, durationMs, codes, timeoutMs }) => {
  try {
    const s = await fetch('/api/auth/session', { cache: 'no-store' });
    const session = s.ok ? await s.json() : {};
    if (!session.accessToken) return { stage: 'session', status: s.ok ? 401 : s.status, signedOut: s.ok, cfMitigated: s.headers.get('cf-mitigated') };
    const bytes = Uint8Array.from(atob(audio), (c) => c.charCodeAt(0));
    const form = new FormData();
    form.append('file', new Blob([bytes], { type: 'audio/wav' }), 'codex.wav');
    form.append('dictation_session_id', sessionId);
    form.append('attempt_id', crypto.randomUUID());
    form.append('duration_ms', durationMs);
    const headers = {
      authorization: 'Bearer ' + session.accessToken,
      'oai-language': navigator.language,
      originator: 'Codex Browser',
      'x-openai-web-frontend': 'codex_webview',
      'x-openai-codex-window-type': 'browser',
    };
    if (session.account?.id) headers['chatgpt-account-id'] = session.account.id;
    const did = document.cookie.match(/(?:^|; )oai-did=([^;]*)/);
    if (did) headers['oai-device-id'] = headers['oai-did'] = decodeURIComponent(did[1]);
    const r = await fetch('/backend-api/transcribe', { method: 'POST', headers, body: form, referrer: '/', signal: AbortSignal.timeout(timeoutMs) });
    const body = await r.text();
    let json = null;
    try { json = JSON.parse(body); } catch {}
    const out = { stage: 'transcribe', status: r.status, cfMitigated: r.headers.get('cf-mitigated') };
    if (r.ok && typeof json?.text === 'string') out.text = json.text;
    else out.code = codes.find((c) => body.includes(c))
      ?? json?.detail?.code ?? (typeof json?.detail === 'string' ? json.detail.slice(0, 200) : null);
    return out;
  } catch (e) {
    return { stage: 'network', status: 0, code: String(e?.name || e) };
  }
}"#;

/// The result of an upload.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub ok: bool,
    pub stage: String,
    pub status: u16,
    pub text: Option<String>,
    pub code: Option<String>,
    pub cf_mitigated: Option<String>,
    pub message: String,
    pub retry: bool,
    pub challenge: bool,
    pub show_window: bool,
    pub attempts: u32,
    pub page: Option<Kind>,
}

fn upload_code(recording: &Recording, session_id: &str) -> String {
    let args = json!({
        "audio": base64::engine::general_purpose::STANDARD.encode(&recording.wav),
        "sessionId": session_id,
        "durationMs": recording.duration_ms.to_string(),
        "codes": MESSAGES.map(|(code, _)| code),
        "timeoutMs": TIMEOUT_MS,
    });
    format!("({UPLOAD})({args})")
}

/// Reads the answer of the page. The page data is not trusted, so a missing or wrong field gets its default.
fn classify(raw: &Value) -> Outcome {
    let text = |key: &str| raw.get(key).and_then(Value::as_str).map(String::from);
    let mut out = Outcome {
        stage: text("stage").unwrap_or_default(),
        status: raw.get("status").and_then(Value::as_u64).unwrap_or(0) as u16,
        text: text("text"),
        code: text("code"),
        cf_mitigated: text("cfMitigated"),
        ..Outcome::default()
    };
    let known = out.code.as_deref().and_then(|code| MESSAGES.iter().find(|(c, _)| *c == code)).map(|(_, m)| *m);
    if out.text.is_some() {
        out.ok = true;
    } else if let Some(message) = known {
        out.message = message.into();
    } else if out.status == 401 {
        out.message = "The ChatGPT session expired. Use \"Sign in to ChatGPT\".".into();
    } else if out.status == 403 && out.cf_mitigated.is_some() {
        out.retry = true;
        out.challenge = true;
        out.message = CHALLENGE.into();
    } else if out.status == 429 {
        out.message = "ChatGPT limits the requests (HTTP 429). Try again later.".into();
    } else {
        out.retry = out.status == 0 || out.status >= 500;
        let reason = out.code.clone().unwrap_or_else(|| format!("HTTP {}", out.status));
        out.message = format!("The transcription failed ({reason}).");
    }
    out
}

/// One upload, and a maximum of one automatic retry. A retry gets a new attempt_id.
pub async fn transcribe(app: &AppHandle, recording: &Recording) -> Outcome {
    let code = upload_code(recording, &uuid::Uuid::new_v4().to_string());
    let once = || async {
        if !on_chatgpt(app) {
            start(app).await;
        }
        match evaluate(app, &code, Duration::from_millis(TIMEOUT_MS + 10_000)).await {
            Ok(raw) => classify(&raw),
            Err(err) => classify(&json!({ "stage": "page", "status": 0, "code": err })),
        }
    };
    let mut result = Outcome { attempts: 1, ..once().await };
    if !result.ok && result.retry {
        if result.challenge && kind(app) != Some(Kind::Light) {
            result.show_window = true;
            return result;
        }
        if result.challenge {
            load(app, HOME).await;
        }
        result = Outcome { attempts: 2, page: kind(app), ..once().await };
        result.show_window = result.challenge;
    }
    result
}
