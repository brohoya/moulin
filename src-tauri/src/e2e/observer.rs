//! Records what the user sees during the test: the states of the tray and of the pop-up, and a picture of the
//! pop-up in each state, with what its page shows at that time.
use super::VIEWS;
use crate::{
    app::{Event, State, moulin},
    platform,
    report::file_name,
    ui::overlay::{self, View},
};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};
use tauri::{AppHandle, WebviewWindow, async_runtime::spawn};
use tokio::sync::{broadcast::error::RecvError, oneshot};

/// A change of the pop-up.
#[derive(Clone, Serialize)]
pub struct Shown {
    pub view: View,
    pub text: String,
    pub visible: bool,
}

/// What the test saw.
#[derive(Clone, Default)]
pub struct Seen {
    pub tray: Vec<State>,
    pub overlay: Vec<Shown>,
    /// The file of the picture of each state of the pop-up, or the reason why it is missing.
    pub pictures: Map<String, Value>,
    /// What the pop-up page showed at the time of each picture.
    pub pages: Map<String, Value>,
}

pub struct Observer(Arc<Mutex<Seen>>);

impl Observer {
    /// Starts to record the events of the app.
    pub fn start(app: &AppHandle, out_dir: &Path, stamp: &str) -> Self {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let mut events = moulin(app).events.subscribe();
        let (app, out_dir, stamp, record) = (app.clone(), out_dir.to_path_buf(), stamp.to_string(), seen.clone());
        spawn(async move {
            loop {
                match events.recv().await {
                    Ok(Event::State(state)) => lock(&record).tray.push(state),
                    Ok(Event::Overlay { view, text, visible }) => {
                        lock(&record).overlay.push(Shown { view, text, visible });
                        // One picture of each state.
                        if visible
                            && VIEWS.contains(&view)
                            && lock(&record).pictures.insert(view.as_str().into(), json!("pending")).is_none()
                        {
                            let file = out_dir.join(format!("e2e-{stamp}-pill-{}.png", view.as_str()));
                            spawn(picture(app.clone(), record.clone(), view, file));
                        }
                    }
                    Err(RecvError::Closed) => return,
                    _ => {}
                }
            }
        });
        Self(seen)
    }

    /// Waits until the pictures are ready, for a maximum of 3 seconds.
    pub async fn wait_for_pictures(&self) {
        for _ in 0..30 {
            if !lock(&self.0).pictures.values().any(|v| v == "pending") {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub fn seen(&self) -> Seen {
        lock(&self.0).clone()
    }
}

fn lock(seen: &Mutex<Seen>) -> MutexGuard<'_, Seen> {
    seen.lock().unwrap()
}

/// Takes a picture of the pop-up, for a visual check of the theme. It waits for the change of width, and in the
/// recording state, for sound in the bars.
async fn picture(app: AppHandle, seen: Arc<Mutex<Seen>>, view: View, file: PathBuf) {
    let delay = if view == View::Recording { 1500 } else { 350 };
    tokio::time::sleep(Duration::from_millis(delay)).await;
    let value = match overlay::window(&app) {
        Some(window) => {
            let state = page_value(&window, "pill").await;
            lock(&seen).pages.insert(view.as_str().into(), state);
            save_picture(&window, &file).await
        }
        None => json!("not taken: no pop-up"),
    };
    lock(&seen).pictures.insert(view.as_str().into(), value);
}

/// What the E2E test reads in the pages (see pages.js).
const PAGES: &str = include_str!("pages.js");

/// Runs a function of pages.js in the page of a window, and returns its value (null if the page gives no answer).
pub async fn page_value(window: &WebviewWindow, function: &str) -> Value {
    let (tx, rx) = oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let sent = window.eval_with_callback(format!("JSON.stringify(({PAGES}).{function}())"), move |json| {
        if let Some(tx) = tx.lock().unwrap().take() {
            let _ = tx.send(json);
        }
    });
    if sent.is_err() {
        return Value::Null;
    }
    match tokio::time::timeout(Duration::from_secs(2), rx).await {
        Ok(Ok(json)) => serde_json::from_str::<String>(&json).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// Saves a picture of the page of a window (macOS only). Returns the file name, or the reason why it is missing.
pub async fn save_picture(window: &WebviewWindow, file: &Path) -> Value {
    match platform::snapshot_png(window).await.and_then(|png| std::fs::write(file, png).map_err(|err| err.to_string())) {
        Ok(()) => json!(file_name(file)),
        Err(err) => json!(format!("not taken: {err}")),
    }
}
