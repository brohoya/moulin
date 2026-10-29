//! Runs code of Moulin in the chatgpt.com page, and reads its result.
//!
//! `eval` gives no result for a promise, so the page keeps the result under a random property until Moulin reads it.
//! The code returns no secret: only a status, a text, or an error code.
use serde_json::Value;
use std::{
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::AppHandle;
use tokio::sync::oneshot;

/// Starts the code, and keeps its result as JSON text in `window[key][id]`.
const START: &str = "(key, id, run) => {
  const box = window[key] || (window[key] = {});
  box[id] = null;
  Promise.resolve().then(run).then(
    (value) => { box[id] = JSON.stringify({ value: value ?? null }); },
    (error) => { box[id] = JSON.stringify({ error: String((error && error.message) || error) }); },
  );
}";

/// Gives the result and deletes it. The result is null while the code runs, and false if a navigation removed it.
const POLL: &str = "(key, id) => {
  const box = window[key];
  if (!box || !(id in box)) return false;
  const value = box[id];
  if (value !== null) delete box[id];
  return value;
}";

/// The name of the page property, as a JSON string.
static KEY: LazyLock<String> = LazyLock::new(|| {
    let key = format!("__moulin_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    serde_json::to_string(&key).unwrap_or_default()
});
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Runs code in the page and returns its value. The code is an expression, and it can give a promise.
pub async fn evaluate(app: &AppHandle, code: &str, limit: Duration) -> Result<Value, String> {
    let window = super::window(app)?;
    let key = KEY.as_str();
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    window.eval(format!("({START})({key}, {id}, () => ({code}))")).map_err(|err| err.to_string())?;
    let poll = format!("({POLL})({key}, {id})");
    let deadline = Instant::now() + limit;
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let (tx, rx) = oneshot::channel();
        let tx = Mutex::new(Some(tx));
        window
            .eval_with_callback(poll.clone(), move |json| {
                if let Some(tx) = tx.lock().unwrap().take() {
                    let _ = tx.send(json);
                }
            })
            .map_err(|err| err.to_string())?;
        if let Ok(Ok(json)) = tokio::time::timeout(Duration::from_secs(5), rx).await {
            match serde_json::from_str::<Value>(&json) {
                Ok(Value::String(text)) => return result(&text),
                Ok(Value::Bool(false)) => return Err("the page changed during the request".into()),
                _ => {}
            }
        }
        if Instant::now() > deadline {
            return Err("the page did not answer".into());
        }
    }
}

/// Reads the result that the page keeps: `{ value }` or `{ error }`.
fn result(text: &str) -> Result<Value, String> {
    let result: Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
    match result.get("error").and_then(Value::as_str) {
        Some(error) => Err(error.to_string()),
        None => Ok(result.get("value").cloned().unwrap_or(Value::Null)),
    }
}
