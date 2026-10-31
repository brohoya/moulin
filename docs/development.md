# Moulin for developers

The open work is in the [GitHub issues](https://github.com/brohoya/moulin/issues).

## Repository

| Folder | Content |
| --- | --- |
| [src-tauri/](../src-tauri/) | The app: the Rust code, the Tauri configuration, and the icons. |
| [src/](../src/) | The web pages of the app: the pop-up and the settings window. |
| [website/](../website/) | The website at https://brohoya.github.io/moulin/ with its fonts and images, the install script, and the Karabiner rule. [mill.js](../website/mill.js) and [wordmark.js](../website/wordmark.js) have the shapes of the mark: the website draws with them, and `npm run icons` makes all the images from them. |
| [docs/](.) | This guide, and the images of the README. |
| [tests/](../tests/) | The E2E test: the runner, the audio sample and its text, and the script that makes the sample. |
| [scripts/](../scripts/) | The scripts that make the images (`npm run icons`), and the macOS build scripts (`npm run reinstall`, `npm run signing:setup`). |
| [.github/workflows/](../.github/workflows/) | The release, and the publication of the website. |

Each file in `website/` is public: GitHub publishes the folder after each push to `main` that changes it. To see the website on this computer, run `python3 -m http.server -d website 8000` and open http://localhost:8000.

## How it works

Moulin is a [Tauri 2](https://v2.tauri.app) app. The Rust code controls the tray, the shortcut, the recording, the upload, and the paste. The web view of the system shows the pages: WKWebView on macOS, WebView2 on Windows, and WebKitGTK on Linux.

1. Hidden window: Moulin keeps one hidden window with its own data store for chatgpt.com. This window is not the browser of the user. After the sign-in, it shows `https://chatgpt.com/robots.txt`, because the upload needs only the chatgpt.com origin and the cookies. This light page uses about 35 MB of memory, and the full ChatGPT app about 700 MB. On macOS, the window sends the user agent of Safari, because some sign-in pages refuse the default user agent of WKWebView.
2. Recording: Moulin records the microphone with [cpal](https://github.com/RustAudio/cpal) (Core Audio, WASAPI, or ALSA), and makes a WAV file (16 kHz, mono, 16-bit). The recording is native, because WebKit gives the microphone only to a page that has the focus, starts no capture in a hidden window, and hides the names of the microphones until a capture starts. The pop-up never takes the focus.
3. Upload: at the stop, Moulin runs the upload code in the chatgpt.com page. The code reads the access token from `GET /api/auth/session` and sends `POST /backend-api/transcribe` with `fetch`, like the dictation button of the page (see [The transcription request](#the-transcription-request-october-2026)). The token stays in the page. The web view gives no value for a promise, so the page keeps the result under a random property, and Moulin reads it every 100 ms.
4. Paste: Moulin writes the text to the clipboard, then sends Cmd+V (macOS) or Ctrl+V (Windows, Linux).

Shortcut: in toggle mode, the shortcut starts and stops the dictation. In hold mode, the key-down starts it and the key-up stops it. A press shorter than 0.3 s cancels it, so that no empty audio goes to ChatGPT. The global shortcut plugin of Tauri gives both key events on macOS, Windows, and X11, so the shortcut needs no key hook and no Accessibility permission. `Moulin --toggle` starts or stops a dictation, so that a system shortcut can run it. If Moulin does not run, this command starts Moulin and the dictation.

Cloudflare check: Cloudflare protects chatgpt.com from bots. Sometimes it refuses a request with HTTP 403 and the header `cf-mitigated: challenge`, until a page passes a check. The light page is plain text, so it cannot run the check. After a refusal, Moulin loads the full page in the hidden window, waits up to 10 s for a new `cf_clearance` cookie, and sends the request again. After a second refusal, Moulin shows the window, because the check can need a click. It waits up to 2 minutes, or until the user closes the window, and then it sends the request a last time. After a last refusal, the settings window tells the next step, and Moulin keeps the recording.

Data: Moulin keeps `settings.json` in `~/Library/Application Support/app.moulin` (macOS), `%APPDATA%\app.moulin` (Windows), or `~/.config/app.moulin` (Linux). The ChatGPT session is in `~/Library/WebKit/app.moulin/WebsiteDataStore` (macOS), `%LOCALAPPDATA%\app.moulin\chatgpt` (Windows), or `~/.local/share/app.moulin/chatgpt` (Linux).

Security:

- The chatgpt.com page gets no permission (Moulin denies the microphone) and no IPC. Tauri accepts commands only from the pages of Moulin, and each window can call only its own commands ([src-tauri/capabilities/](../src-tauri/capabilities/) and the app manifest in [src-tauri/build.rs](../src-tauri/build.rs)).
- Tauri adds `window.isTauri` and `window.__TAURI_INTERNALS__` to each page, also to chatgpt.com. So the page can see that it runs in a Tauri app.

The Rust code in [src-tauri/src/](../src-tauri/src/):

| Path | What it does |
| --- | --- |
| [main.rs](../src-tauri/src/main.rs) | The start: the plugins, the commands, the windows, and the start-up sequence. |
| [app.rs](../src-tauri/src/app.rs) | The shared state, and the status that the tray, the pop-up, and the settings window show. |
| [dictation.rs](../src-tauri/src/dictation.rs) | A dictation: the checks, the recording, the upload, the clipboard, and the paste. |
| [hotkey.rs](../src-tauri/src/hotkey.rs) | The global shortcut, and Esc during a recording. |
| [account.rs](../src-tauri/src/account.rs) | The check of the ChatGPT session, the sign-in, and the sign-out. |
| [ui/](../src-tauri/src/ui/) | The tray icon and its menu, the pop-up, and the settings window with its commands. |
| [chatgpt/](../src-tauri/src/chatgpt/) | The hidden chatgpt.com window, the bridge that runs code in its page, the upload, and the Cloudflare check. |
| [audio/](../src-tauri/src/audio/) | The recorder, the resampler, and the WAV files. |
| [paste.rs](../src-tauri/src/paste.rs) | The paste on each system. |
| [platform/](../src-tauri/src/platform/) | The parts that are different on each system: the pop-up window, the macOS permissions, and the login item. |
| [settings.rs](../src-tauri/src/settings.rs), [options.rs](../src-tauri/src/options.rs) | The settings file, and the command line options. |
| [e2e/](../src-tauri/src/e2e/), [capture.rs](../src-tauri/src/capture.rs), [report.rs](../src-tauri/src/report.rs) | The test modes, and their report files. |

The code that runs in a page is in a `.js` file next to the Rust file that uses it, for example [upload.js](../src-tauri/src/chatgpt/upload.js) next to upload.rs. Each file holds an object of functions. Rust reads the file with `include_str!` and calls a function by its name, with the arguments as JSON (`chatgpt::call`). Do not write JavaScript in Rust strings.

## Build and test

You need Node.js, Rust (install it with [rustup](https://rustup.rs)), and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) of your system. On Linux, also install `libasound2-dev` for the microphone.

| Command | What it does |
| --- | --- |
| `npm start` | Runs Moulin from the sources (`tauri dev`). Quit the installed Moulin first: both use the same settings file and the same shortcut. |
| `npm run signing:setup` | macOS, one time: makes a local signing certificate in `~/Library/Keychains/moulin-signing.keychain-db`. |
| `npm run reinstall` | macOS: builds the app, signs it with the local certificate, replaces `~/Applications/Moulin.app`, and starts it. macOS ties the permissions to the bundle ID and the certificate, so it keeps them. If the certificate changed, the script removes the old permissions, so that macOS asks again. |
| `npm run dist:mac`, `dist:win`, `dist:linux` | Builds the installers in `src-tauri/target/release/bundle/`. The macOS build has an ad-hoc signature, so macOS asks again for the permissions after each install. |
| `npm run e2e` | The E2E test (see below) in the installed app, or else in the build in `src-tauri/target`. It quits a running Moulin, and starts it again at the end. |
| `npm run e2e:dev` | The same test from the sources. |
| `npm run e2e -- --hold` | The E2E test in hold mode (macOS, with a shortcut from F1 to F20). |
| `npm run e2e -- --challenge` | The E2E test with a fake Cloudflare refusal of the first upload. |
| `npm run capture` | Shows the ChatGPT window. Do one dictation with the button of the page: Moulin writes the names and the shapes of the request (not the values) to `artifacts/capture-<time>.json`. |
| `npm run icons` | Makes all the images from [website/mill.js](../website/mill.js): the app icon and the tray images in `src-tauri/icons/`, the icon of the settings window, and the images of the website and the README. |
| `npm run sample` | Makes `tests/fr-sample.wav` from `tests/fr-sample.txt` with `say` and ffmpeg. |

The E2E test does one full dictation: start, stop, upload, and clipboard. The recorder plays the sample one time instead of the microphone. A second Moulin process with `--toggle` starts and stops the dictation, like a system shortcut. The test never pastes. It checks the HTTP status, the word error rate (maximum 0.25), the clipboard, and the states of the tray and the pop-up. On macOS, it also checks that the pop-up shows at the bottom center of the screen that the user works on. It writes a report and the recording to `artifacts/`. It needs a signed-in session and ffmpeg, and it replaces the content of the clipboard.

## Release

1. Set `version` in [package.json](../package.json), and commit. The settings window shows this number.
2. Push `main`.
3. Push a tag with the same version: `git tag v1.0.0 && git push origin v1.0.0`.

The workflow [release.yml](../.github/workflows/release.yml) builds Moulin on macOS, Windows, and Linux, and publishes the release in about 8 minutes. The file names do not include the version, so the website, the README, and `install.sh` always give the latest release.

- To test the builds without a release, run the workflow by hand: Actions > Release > Run workflow. The files stay on the page of the run.
- To delete a release and its tag, run `gh release delete v1.0.0 --cleanup-tag --yes`, then `git tag -d v1.0.0`.

## Troubleshooting

- On macOS 26, when a terminal starts the binary `Moulin.app/Contents/MacOS/Moulin`, macOS puts the menu bar icon in the group of the terminal app. System Settings cannot remove this link ([CodexBar issue 1440](https://github.com/steipete/CodexBar/issues/1440) gives a repair). Start Moulin from Finder, from Spotlight, or with `open`.
- macOS protects `~/Documents`. A new build with an ad-hoc signature shows a privacy prompt when it first reads a file there (for example the E2E sample), and it waits for the answer. A build from `npm run reinstall` keeps the permission of the earlier builds.
- If a key remapper changes the key (for example the Karabiner rule that changes F5 into F13), open Settings, click the shortcut, and push the key. Moulin keeps the key that it gets. In the shortcut field, Backspace removes the shortcut of Moulin.
- On Wayland, Moulin has no global shortcut, because the shortcut plugin of Tauri uses X11. Add a system shortcut that runs `moulin --toggle` (GNOME: Keyboard > Custom Shortcuts, KDE: Shortcuts > Add New > Command). The desktop can also put the pop-up in another place, and GNOME can refuse the clipboard to an app in the background. Most Linux terminals paste with Ctrl+Shift+V, so the automatic Ctrl+V does not paste there.
- A dictation stops after 5 minutes, because longer uploads are not tested.

## The transcription request (October 2026)

Measured with ChatGPT Plus on macOS 26.5. `npm run capture` records the request of the dictation button. It sees the headers that the page sets, not the headers that the web view adds.

- Request: `POST https://chatgpt.com/backend-api/transcribe` over HTTP/2.
- Form fields: `file`, `dictation_session_id`, `attempt_id`, and `duration_ms`. The page sends `codex.webm` (type `audio/webm;codecs=opus`), and Moulin sends `codex.wav`. The page sends no `language` field.
- Headers that the page sets: `authorization`, `chatgpt-account-id`, `oai-device-id`, `oai-did`, `oai-language`, `originator`, `x-openai-web-frontend`, `x-openai-codex-window-type`, `x-openai-rate-limit-join-workspace`, `content-type`, and `referer`.
- Sources of the values: `authorization` is the `accessToken` of `GET /api/auth/session`, and `chatgpt-account-id` is its `account.id`. `oai-device-id` and `oai-did` are the `oai-did` cookie, which is not HttpOnly. Moulin sends the same constant values for `originator`, `x-openai-web-frontend`, and `x-openai-codex-window-type`. It does not send `x-openai-rate-limit-join-workspace`, because its value is unknown.
- The request has no sentinel, proof-of-work, or Turnstile header. The page opens no `wss://chatgpt.com/backend-api/dictation/stream`.
- Response: `{"text": "<string>", "asset_pointer": "<string>", "asset_ttl": "<string>", "asset_format": "<string>"}`.

| Test | Result |
| --- | --- |
| The field `file` alone | The server accepts it. |
| WAV audio | HTTP 200. |
| `language=en` with French audio | French text: the server ignores `language`. |
| A fetch from the page | Cloudflare refuses it only rarely (see "Cloudflare check"). |
| 101 s of speech | HTTP 200 in 5.2 s, with no missing text. |
| 151 s, five repeats of the sample | One repeat is missing in the text. |
| `usage_limit_reached`, `usage_not_included` | Not seen. Moulin shows a message for each. |
| Word error rate on the sample | Moulin: 0.047 to 0.071. The dictation button of the page: 0.063. |
| From the stop to the clipboard, 30 s of audio | 1.4 to 1.7 s. |
| Memory of the Moulin process, idle | 103 to 106 MB. The WebKit processes are system services, so `ps` does not show them as processes of Moulin. |
