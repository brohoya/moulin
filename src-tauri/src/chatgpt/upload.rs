//! The upload of a recording: the page sends it to /backend-api/transcribe, and Moulin reads the result.
use super::{
    Kind,
    check::{self, Step},
    evaluate, kind, on_chatgpt, start,
};
use crate::audio::Recording;
use base64::Engine;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
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
    /// Cloudflare refused the request (see check.rs).
    pub challenge: bool,
    pub attempts: u32,
    pub page: Option<Kind>,
}

/// The E2E test: the next upload gets a refusal of Cloudflare, and sends no request.
static FAKE_REFUSAL: AtomicBool = AtomicBool::new(false);

/// The E2E test: the next upload gets a refusal of Cloudflare without a request, so that Moulin runs the check.
pub fn fake_refusal() {
    FAKE_REFUSAL.store(true, Ordering::Relaxed);
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
        out.message = check::UPLOAD_REFUSED.into();
    } else if out.status == 429 {
        out.message = "ChatGPT limits the requests (HTTP 429). Try again later.".into();
    } else {
        out.retry = out.status == 0 || out.status >= 500;
        let reason = out.code.clone().unwrap_or_else(|| format!("HTTP {}", out.status));
        out.message = format!("The transcription failed ({reason}).");
    }
    out
}

/// One upload. A network or server error gets one more attempt. After a refusal of Cloudflare, each step of the check
/// gets one more attempt. `on_check` gets each step before it starts, and None when the upload starts again after the
/// step. Each attempt gets a new attempt_id.
pub async fn transcribe(app: &AppHandle, recording: &Recording, on_check: impl Fn(Option<Step>)) -> Outcome {
    let code = &upload_code(recording, &uuid::Uuid::new_v4().to_string());
    let attempt = async |attempts: u32| {
        if !on_chatgpt(app) {
            start(app).await;
        }
        let outcome = if FAKE_REFUSAL.swap(false, Ordering::Relaxed) {
            classify(&json!({ "stage": "transcribe", "status": 403, "cfMitigated": "challenge" }))
        } else {
            match evaluate(app, code, Duration::from_millis(TIMEOUT_MS + 10_000)).await {
                Ok(raw) => classify(&raw),
                Err(err) => classify(&json!({ "stage": "page", "status": 0, "code": err })),
            }
        };
        Outcome { attempts, page: kind(app), ..outcome }
    };
    let mut result = attempt(1).await;
    if result.retry && !result.challenge {
        result = attempt(2).await;
    }
    for step in Step::ALL {
        if !result.challenge {
            break;
        }
        on_check(Some(step));
        check::run(app, step).await;
        on_check(None);
        result = attempt(result.attempts + 1).await;
    }
    result
}
