# Moulin for developers

The open work is in the [GitHub issues](https://github.com/brohoya/moulin/issues).

## How it works

Moulin is a [Tauri 2](https://v2.tauri.app) app with two parts. The Rust code in [src-tauri/](../src-tauri/) controls the tray, the shortcut, the recording, the upload, and the paste. The web pages in [src/](../src/) are the pop-up and the settings window. The web view of the system shows the pages: WKWebView on macOS, WebView2 on Windows, and WebKitGTK on Linux.

1. Hidden browser: Moulin keeps one hidden window with the web view of the system and a separate data store for chatgpt.com. This is not Safari, Chrome, or your default browser. On macOS, the window sends the user agent of Safari, because the engine is WebKit. After the sign-in, the window shows `https://chatgpt.com/robots.txt`, not the ChatGPT app. The upload needs only the chatgpt.com origin and the cookies.
2. Audio: Moulin records the microphone with [cpal](https://github.com/RustAudio/cpal) (Core Audio, WASAPI, or ALSA), and makes a WAV file (16 kHz, mono, 16-bit). The pop-up shows the loudness that the recorder sends. Moulin keeps the audio until the text is in the clipboard.
3. Upload: at the stop, Moulin runs the upload code in the chatgpt.com page. The code reads the access token from `GET /api/auth/session` and sends `POST /backend-api/transcribe` with `fetch`. The form fields and the headers are the same as the request of the dictation button. The file is WAV (`codex.wav`) instead of WebM. The token stays in the page. The web view gives no value for a promise, so the page keeps the result under a random property, and Moulin reads it every 100 ms.
4. Completion: the resolved `fetch` is the completion signal. Moulin writes the `text` field to the clipboard, then sends Cmd+V (macOS) or Ctrl+V (Windows, Linux). Moulin never reads the message box of ChatGPT.

Native recording: WebKit gives the microphone only to a page that has the focus, it starts no capture in a hidden window, and it hides the names of the microphones until a capture starts. The pop-up never takes the focus, and it stays hidden when "Show the pop-up" is off. So Moulin records the audio itself, and the settings get the list of microphones from the system.

Hold mode: the global shortcut plugin of Tauri reports the key-down and the key-up of the shortcut (macOS, Windows, and X11). The key-down starts the dictation, and the key-up stops it. A press shorter than 0.3 s cancels the dictation, so that no empty audio goes to ChatGPT. The hold mode needs no key hook and no Accessibility permission.

Code:

| Path | What it does |
| --- | --- |
| [src-tauri/src/main.rs](../src-tauri/src/main.rs) | The start: the plugins, the commands, the windows, and the start-up sequence. |
| [app.rs](../src-tauri/src/app.rs) | The shared state, and the status that the tray, the pop-up, and the settings window show. |
| [dictation.rs](../src-tauri/src/dictation.rs) | A dictation: the checks, the recording, the upload, the clipboard, and the paste. |
| [hotkey.rs](../src-tauri/src/hotkey.rs) | The global shortcut, and Esc during a recording. |
| [account.rs](../src-tauri/src/account.rs) | The check of the ChatGPT session, the sign-in, and the sign-out. |
| [ui/](../src-tauri/src/ui/) | The tray icon and its menu, the pop-up, and the settings window with its commands. |
| [chatgpt/](../src-tauri/src/chatgpt/) | The hidden chatgpt.com window, the bridge that runs code in its page, and the upload. |
| [audio/](../src-tauri/src/audio/) | The recorder, the resampler, and the WAV files. |
| [paste.rs](../src-tauri/src/paste.rs) | The paste on each system. |
| [platform/](../src-tauri/src/platform/) | The parts that differ for each system: the pop-up window, the macOS permissions, and the login item. |
| [settings.rs](../src-tauri/src/settings.rs), [options.rs](../src-tauri/src/options.rs) | The settings file, and the command line options. |
| [e2e/](../src-tauri/src/e2e/), [capture.rs](../src-tauri/src/capture.rs), [report.rs](../src-tauri/src/report.rs) | The test modes, and their report files. |
| [src/overlay/](../src/overlay/), [src/settings/](../src/settings/) | The pages of the pop-up and of the settings window. |
| [docs/](.) | The website, and the shapes of the mark. |

Data: Moulin keeps `settings.json` in the config folder of the app: `~/Library/Application Support/app.moulin` (macOS), `%APPDATA%\app.moulin` (Windows), or `~/.config/app.moulin` (Linux). The ChatGPT session is in `~/Library/WebKit/app.moulin/WebsiteDataStore` (macOS), `%LOCALAPPDATA%\app.moulin\chatgpt` (Windows), or `~/.local/share/app.moulin/chatgpt` (Linux). Only your user account can read the files.

Security:

- The chatgpt.com page gets no permission (Moulin denies the microphone) and no IPC: Tauri accepts commands only from the pages of Moulin. Each window can call only its own commands ([src-tauri/capabilities/](../src-tauri/capabilities/) and the app manifest in [src-tauri/build.rs](../src-tauri/build.rs)).
- Tauri adds `window.isTauri` and `window.__TAURI_INTERNALS__` to each page, also to chatgpt.com. So the page can see that it runs in a Tauri app.

## Build and test

You need Node.js, Rust (install it with [rustup](https://rustup.rs)), and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) of your system. On Linux, also install `libasound2-dev` for the microphone.

- macOS: run `npm run signing:setup` one time, then `npm run reinstall`. The second command builds the app (about 80 seconds, more for the first build), signs it with a local certificate, replaces `~/Applications/Moulin.app`, and starts it. macOS ties the permissions to the bundle ID and the certificate, so it keeps them after each reinstall. `npm run dist:mac` makes `Moulin.app` and a dmg with an ad-hoc signature instead: after an install of such a build, macOS asks for the permissions again.
- Windows: `npm run dist:win` makes the installer in `src-tauri/target/release/bundle/nsis`.
- Linux: `npm run dist:linux` makes the AppImage in `src-tauri/target/release/bundle/appimage`.

| Command | What it does |
| --- | --- |
| `npm start` | Runs Moulin from the sources (`tauri dev`). Quit the installed Moulin first: both use the same settings file and the same shortcut. |
| `npm run e2e` | The E2E test in the installed (or built) app. It quits a running Moulin and starts it again after. |
| `npm run e2e:dev` | The same test from the sources. |
| `npm run e2e -- --hold` | The E2E test in hold mode (macOS, a shortcut from F1 to F20): it holds the key of the shortcut during the sample, then releases it. |
| `npm run dist:mac`, `dist:win`, `dist:linux` | Builds the installers in `src-tauri/target/release/bundle/`. The release workflow gives them the names of the downloads. |
| `npm run reinstall` | macOS: builds the app, signs it with the local certificate, replaces `~/Applications/Moulin.app`, and starts it. If the signature identity changed, it removes the old permissions, so that macOS asks again with clean entries. |
| `npm run signing:setup` | macOS, one time: creates the local signing certificate in `~/Library/Keychains/moulin-signing.keychain-db`. The login keychain keeps only the password of that keychain. |
| `npm run capture` | Shows the ChatGPT window. Do one dictation with the button of the page. It writes `artifacts/capture-<time>.json` with names and shapes only. |
| `npm run icons` | Makes all the images from the shapes in [docs/mill.js](mill.js): the app icon and the tray images in `src-tauri/icons`, the icon of the settings window, and the images of the website and the README. |
| `npm run sample` | Makes `fixtures/fr-sample.wav` (48 kHz, mono, 16-bit) from `fixtures/fr-sample.txt` with `say` and ffmpeg. |

The E2E test does one full dictation: start, stop, upload, clipboard. A second Moulin process with `--toggle` starts and stops the dictation, the same path as a system shortcut. On macOS, `npm run e2e` starts the app with `open`, so that macOS does not put the menu bar icon in the group of the terminal app. The recorder plays the sample WAV one time instead of the microphone, then gives silence. The test never pastes. It writes `artifacts/e2e-<time>.json`, the recording `artifacts/e2e-<time>.wav`, and a picture of the pop-up in each state (macOS only, a WebKit snapshot: the picture is empty while the screen is locked). It checks the HTTP status, the word error rate (maximum 0.25), the clipboard, the tray states, the pop-up states (also inside the pop-up page, with its timer), and the silence after the end of the sample. It needs a signed-in session and ffmpeg, and it replaces the clipboard content. In hold mode, the test posts its key events from a private event source: an event of the HID system state can get a key-up from macOS at the next input of the user, because no physical key is down.

The folder `docs` is also the website (GitHub Pages, branch `main`, folder `/docs`). To see it on this computer, run `python3 -m http.server -d docs 8000`. [docs/install.sh](install.sh) is the install command.

## Release

1. Set `version` in [package.json](../package.json), and commit.
2. Push a tag with the same version: `git tag v1.0.1 && git push origin v1.0.1`.

The workflow [.github/workflows/release.yml](../.github/workflows/release.yml) builds Moulin on macOS, Windows, and Linux, and publishes the release. The file names do not include the version, so the links of the website and of the README always give the latest release.

## Troubleshooting

- On macOS 26, when a terminal starts the binary `Moulin.app/Contents/MacOS/Moulin`, macOS puts the menu bar icon in the group of the terminal app and keeps it there. The System Settings toggle cannot remove this link ([CodexBar issue 1440](https://github.com/steipete/CodexBar/issues/1440) gives the repair of `trackedApplications`). Start Moulin from Finder, Spotlight, or `open`.
- If a key remapper changes the key (for example a Karabiner-Elements rule that turns F5 into F13), open Settings, click the shortcut, and push the key: Moulin keeps the key that it gets.
- `Moulin --toggle` starts or stops a dictation, so a system shortcut can run it. If Moulin does not run yet, this command starts Moulin and the dictation. In the settings, Backspace in the shortcut field removes Moulin's own shortcut.
- On Wayland, Moulin has no global shortcut: the shortcut plugin of Tauri uses X11. Add a system shortcut that runs `moulin --toggle` (GNOME: Keyboard > Custom Shortcuts, KDE: Shortcuts > Add New > Command). On Wayland, the desktop can also put the pop-up in another place than the bottom of the screen, and GNOME can refuse the clipboard to a background app. Most Linux terminals paste with Ctrl+Shift+V, so the automatic Ctrl+V does not paste there.
- macOS protects `~/Documents`. A new build with an ad-hoc signature shows a privacy prompt when it first reads a file there (for example the E2E sample), and it waits until you answer. A build that `npm run reinstall` signs gets the permission of the earlier builds.
- A dictation stops after 5 minutes, because longer uploads are not tested.

## Measurements (October 2026)

Tauri 2.12.5, WebKit 21624, macOS 26.5, ChatGPT Plus. Evidence: `artifacts/e2e-2026-10-29T01-28-35-300Z.json`, `artifacts/e2e-2026-10-29T02-13-45-625Z.json`, and `artifacts/e2e-2026-10-29T10-46-14-747Z.json` (E2E runs of the build in `src-tauri/target`).

| Item | Result |
| --- | --- |
| `Moulin.app` | 6.0 MB |
| Download for macOS (zip) | 2.6 MB |
| E2E test (3 runs) | Pass |
| Stop to clipboard, 30 s of audio | 1.4 to 1.7 s |
| Word error rate | 0.047 to 0.071 |
| Memory of the Moulin process, idle | 103 to 106 MB |
| Memory of the light page (`robots.txt`) | 33 to 34 MB |
| Memory of the full ChatGPT app | 691 to 727 MB |

The memory is the resident memory. The processes of WebKit (pages, network, GPU) are system services, so `ps` does not show them as processes of Moulin.

## The transcription request (October 2026)

ChatGPT Plus, macOS 26.5. Evidence: `artifacts/capture-*.json`, `artifacts/probes-2026-10-27.json`, and `artifacts/e2e-*.json`.

`npm run capture` wraps `fetch` at the start of the chatgpt.com page and records the request of the dictation button. It sees the headers that the page sets, not the headers that the web view adds. It writes the names and the shapes, not the values.

- Request: `POST https://chatgpt.com/backend-api/transcribe` over HTTP/2.
- Form fields: `file`, `dictation_session_id`, `attempt_id`, `duration_ms`. The page sends `codex.webm` (type `audio/webm;codecs=opus`), and Moulin sends `codex.wav`. The page sends no `language` field.
- Headers that the page sets: `authorization`, `chatgpt-account-id`, `oai-device-id`, `oai-did`, `oai-language`, `originator`, `x-openai-web-frontend`, `x-openai-codex-window-type`, `x-openai-rate-limit-join-workspace`, `content-type`, `referer`.
- The request has no sentinel, proof-of-work, or Turnstile header.
- Response: `{"text": "<string>", "asset_pointer": "<string>", "asset_ttl": "<string>", "asset_format": "<string>"}`.
- Sources of the values: `authorization` is `accessToken` of `GET /api/auth/session`. `chatgpt-account-id` is `account.id` of the same JSON. `oai-device-id` and `oai-did` are the `oai-did` cookie, which is not HttpOnly.
- The page did not open `wss://chatgpt.com/backend-api/dictation/stream`.

Moulin sends the same constant values for `originator`, `x-openai-web-frontend`, and `x-openai-codex-window-type`. It does not send `x-openai-rate-limit-join-workspace`, because its value is unknown.

| Fact | Result |
| --- | --- |
| The server accepts the field `file` alone | True. |
| WAV works | True (HTTP 200). |
| The backend ignores `language` | True: `language=en` with French audio gave French text. |
| No sentinel, proof-of-work, or Turnstile token | True. |
| A fetch from the page gets no Cloudflare 403 | True on the light page. A request from outside the page is not tested, because all requests must come from the page. |
| Long audio | 101 s of speech: HTTP 200 in 5.2 s, no missing text. 151 s made of five repeats of the sample: one repeat is missing in the text. |
| `usage_limit_reached`, `usage_not_included` | Not seen. Moulin shows a message for each. |
| Word error rate of the dictation button of the page | 0.063, with the same sample. |
