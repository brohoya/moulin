# Moulin for developers

The open work is in the [GitHub issues](https://github.com/brohoya/moulin/issues).

## How it works

1. Hidden browser: Moulin keeps one hidden `BrowserWindow` in the partition `persist:chatgpt`. This is the Chromium engine inside Electron (Chromium 156), not Chrome, Brave, or your default browser. The session sends the user agent of Chrome for the platform. After the sign-in, the window shows `https://chatgpt.com/robots.txt`, not the ChatGPT app. The upload needs only the chatgpt.com origin and the cookies.
2. Audio: the pop-up page records the microphone with `MediaRecorder` (webm/opus, mono, 32 kbit/s). Only the pages of Moulin get the microphone. Moulin keeps the audio until the text is in the clipboard.
3. Upload: at the stop, the main process gives the audio to the chatgpt.com page with `executeJavaScriptInIsolatedWorld`. The code in the isolated world reads the access token from `GET /api/auth/session` and sends `POST /backend-api/transcribe` with `fetch`. The form fields and the headers are the same as the request of the dictation button. The token stays in the page.
4. Completion: the resolved `fetch` is the completion signal. Moulin writes the `text` field to the clipboard, then sends Cmd+V (macOS) or Ctrl+V (Windows, Linux). Moulin never reads the message box of ChatGPT.

Code: [src/main.js](../src/main.js) (tray, shortcut, flow, settings), [src/chatgpt.js](../src/chatgpt.js) (hidden window, sign-in, upload), [src/overlay/](../src/overlay/) (pop-up and recorder), [src/settings/](../src/settings/) (settings window), [src/paste.js](../src/paste.js) (paste on each platform), [docs/](.) (website, and the shapes of the mark).

Data: Moulin keeps `settings.json` and the ChatGPT session (`Partitions/chatgpt`) in `~/Library/Application Support/Moulin` (macOS), `%APPDATA%\Moulin` (Windows), or `~/.config/Moulin` (Linux). The cookies are not encrypted. Only your user account can read the files.

The packaged app turns off the Electron fuses `RunAsNode`, `NODE_OPTIONS`, and the inspect arguments, so that other programs cannot run the Moulin binary as Node with its microphone and Accessibility permissions.

## Build and test

- macOS: run `npm run signing:setup` one time, then `npm run reinstall`. The second command builds the app (about 25 seconds), signs it with a local certificate, replaces `~/Applications/Moulin.app`, and starts it. macOS ties the permissions to the bundle ID and the certificate, so it keeps them after each reinstall. `npm run dist:mac` makes a dmg and a zip with an ad-hoc signature instead: after an install of such a build, macOS asks for the permissions again.
- Windows: `npm run dist:win` makes `dist/Moulin-windows-x64.exe`.
- Linux: `npm run dist:linux` makes `dist/Moulin-linux-x86_64.AppImage`.

| Command | What it does |
| --- | --- |
| `npm start` | Runs Moulin from the sources. Quit the installed Moulin first: both use the same profile. |
| `npm run e2e` | The E2E test in the installed (or built) app. It quits a running Moulin and starts it again after. |
| `npm run e2e:dev` | The same test from the sources. |
| `npm run dist:mac`, `dist:win`, `dist:linux` | Builds the installers in `dist/`. |
| `npm run reinstall` | macOS: builds the app, signs it with the local certificate, replaces `~/Applications/Moulin.app`, and starts it. If the signature identity changed, it removes the old permissions, so that macOS asks again with clean entries. |
| `npm run signing:setup` | macOS, one time: creates the local signing certificate in `~/Library/Keychains/moulin-signing.keychain-db`. The login keychain keeps only the password of that keychain. |
| `npm run capture` | Shows the ChatGPT window. Do one dictation with the button of the page. It writes `artifacts/capture-<time>.json` with names and shapes only. |
| `npm run icons` | Makes all the images from the shapes in [docs/mill.js](mill.js): the app icon, the tray images, and the images of the website and the README. |
| `npm run sample` | Makes `fixtures/fr-sample.wav` (48 kHz, mono, 16-bit) from `fixtures/fr-sample.txt` with `say` and ffmpeg. |

The E2E test does one full dictation: start, stop, upload, clipboard. A second Moulin process with `--toggle` starts and stops the dictation, the same path as a system shortcut. On macOS, `npm run e2e` starts the app with `open`, so that macOS does not put the menu bar icon in the group of the terminal app. The microphone is the sample WAV (Chromium fake device, `%noloop`). The test never pastes. It writes `artifacts/e2e-<time>.json`, the recording `artifacts/e2e-<time>.webm`, and a picture of the pop-up in each state. It checks the HTTP status, the word error rate (maximum 0.25), the clipboard, the tray states, the pop-up states, and the silence after the end of the sample. It needs a signed-in session and ffmpeg, and it replaces the clipboard content. The test modes turn off the sandbox of the Chromium audio service (`--disable-features=AudioServiceSandbox`), because that sandbox blocks the read of the WAV file. The normal app keeps the sandbox.

The folder `docs` is also the website (GitHub Pages, branch `main`, folder `/docs`). To see it on this computer, run `python3 -m http.server -d docs 8000`. [docs/install.sh](install.sh) is the install command.

## Release

1. Set `version` in [package.json](../package.json), and commit.
2. Push a tag with the same version: `git tag v1.0.1 && git push origin v1.0.1`.

The workflow [.github/workflows/release.yml](../.github/workflows/release.yml) builds Moulin on macOS, Windows, and Linux, and publishes the release. The file names do not include the version, so the links of the website and of the README always give the latest release.

## Troubleshooting

- On macOS 26, when a terminal starts the binary `Moulin.app/Contents/MacOS/Moulin`, macOS puts the menu bar icon in the group of the terminal app and keeps it there. The System Settings toggle cannot remove this link ([CodexBar issue 1440](https://github.com/steipete/CodexBar/issues/1440) gives the repair of `trackedApplications`). Start Moulin from Finder, Spotlight, or `open`.
- If a key remapper changes the key (for example a Karabiner-Elements rule that turns F5 into F13), open Settings, click the shortcut, and push the key: Moulin keeps the key that it gets.
- `Moulin --toggle` starts or stops a dictation, so a system shortcut can run it. If Moulin does not run yet, this command starts Moulin and the dictation. In the settings, Backspace in the shortcut field removes Moulin's own shortcut.
- Wayland lets an app use a global shortcut only through the desktop portal (KDE, GNOME 48 or later, Hyprland). On other desktops, add a system shortcut that runs `moulin --toggle` (GNOME: Keyboard > Custom Shortcuts, KDE: Shortcuts > Add New > Command). On Wayland, the desktop can also put the pop-up in another place than the bottom of the screen, and GNOME can refuse the clipboard to a background app. Most Linux terminals paste with Ctrl+Shift+V, so the automatic Ctrl+V does not paste there.
- A dictation stops after 5 minutes, because longer uploads are not tested.

## Research (October 2026)

Electron 45.0.0, macOS 26.5, ChatGPT Plus. Evidence: `artifacts/e2e-*.json`, `artifacts/capture-*.json`, `artifacts/probes-2026-10-27.json`.

| Item | Result |
| --- | --- |
| E2E test (4 runs, the last one in the installed app) | Pass |
| Stop to clipboard, 30 s of audio | 1.4 to 1.8 s |
| Upload of 101 s of speech | 5.2 s, no missing text |
| Word error rate, Moulin (4 runs) | 0.035 to 0.059 |
| Word error rate, dictation button of the page | 0.063 |
| Renderer memory, light page (`robots.txt`) | 88 MB |
| Renderer memory, full ChatGPT app | 557 to 659 MB |

### The request of the dictation button

`npm run capture` records the request with `webContents.debugger` (CDP, Network domain). The values are redacted.

- Request: `POST https://chatgpt.com/backend-api/transcribe` over HTTP/2.
- Form fields: `file` (file name `codex.webm`, type `audio/webm;codecs=opus`), `dictation_session_id`, `attempt_id`, `duration_ms`. The page sends no `language` field.
- Headers that the page sets: `authorization`, `chatgpt-account-id`, `oai-device-id`, `oai-did`, `oai-language`, `originator`, `x-openai-web-frontend`, `x-openai-codex-window-type`, `x-openai-rate-limit-join-workspace`, `content-type`, `referer`.
- Headers that Chromium adds: `cookie`, `origin`, `user-agent`, `accept`, `accept-encoding`, `accept-language`, `content-length`, `priority`, `sec-ch-ua`, `sec-ch-ua-mobile`, `sec-ch-ua-platform`, `sec-fetch-dest`, `sec-fetch-mode`, `sec-fetch-site`.
- The request has no sentinel, proof-of-work, or Turnstile header.
- Response: `{"text": "<string>", "asset_pointer": "<string>", "asset_ttl": "<string>", "asset_format": "<string>"}`.
- Sources of the values: `authorization` is `accessToken` of `GET /api/auth/session`. `chatgpt-account-id` is `account.id` of the same JSON. `oai-device-id` and `oai-did` are the `oai-did` cookie, which is not HttpOnly.
- The page did not open `wss://chatgpt.com/backend-api/dictation/stream`.

Moulin sends the same constant values for `originator`, `x-openai-web-frontend`, and `x-openai-codex-window-type`. It does not send `x-openai-rate-limit-join-workspace`, because its value is unknown. `sec-ch-ua` has no "Google Chrome" brand, so it is different from a real Chrome. Cloudflare did not challenge it.

### Facts from the research

| Fact | Result |
| --- | --- |
| One form field `file`, named `whisper.webm` | Partly true. The page sends four fields and `codex.webm`. The server also accepts `file` alone. |
| WAV also works | True (HTTP 200, word error rate 0.035). |
| The response is `{"text": "..."}` | True, with three more fields (`asset_*`). |
| The backend ignores `language` | True: `language=en` with French audio gave French text. |
| No sentinel, proof-of-work, or Turnstile token | True. |
| A fetch from the page in Electron gets no Cloudflare 403 | True, on the light page and on the full app. The plain curl case is not tested, because all requests must come from the page. |
| `GET /api/auth/session` gives the access token | True. `chatgpt-account-id` is `account.id` of the same JSON. |
| Long audio | 101 s of speech: HTTP 200 in 5.2 s, no missing text. 151 s made of five repeats of the sample: one repeat is missing in the text. |
| `usage_limit_reached`, `usage_not_included` | Not seen. Moulin shows a message for each. |
| Cloudflare challenges the Electron user agent | Not tested. Moulin always sends the Chrome user agent and saw no challenge. |
| Google sign-in works in Electron | Not tested. The sign-in used e-mail and password, and it worked. |
