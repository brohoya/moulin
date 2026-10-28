<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/media/hero-night.svg">
    <img src="docs/media/hero-day.svg" alt="Moulin">
  </picture>
</h1>

<p align="center">Dictation from the menu bar with the speech-to-text of your ChatGPT subscription.<br>
<a href="https://brohoya.github.io/moulin/">Website</a> &nbsp; <a href="#install">Install</a> &nbsp; <a href="#how-it-works">How it works</a></p>

<p align="center"><img src="docs/media/demo.svg" alt="A note on a Mac. The user pushes F5, a dark pop-up shows the recording, the user pushes F5 again, and the sentence appears at the cursor."></p>

The name comes from "moulin à paroles": a person who talks all the time.

1. Push F5 and speak. A small pop-up shows the recording.
2. Push F5 again. Moulin sends the audio to ChatGPT from its own hidden browser.
3. Moulin pastes the text at the cursor. The text also stays in the clipboard.

Moulin works on macOS, Windows, and Linux. Only the macOS build is tested.

## Install

- macOS: run `npm run signing:setup` one time, then `npm run reinstall`. The second command builds the app (about 25 seconds), signs it with a local certificate, replaces `~/Applications/Moulin.app`, and starts it. macOS ties the permissions to the bundle ID and the certificate, so it keeps them after each reinstall. `npm run dist:mac` makes a dmg and a zip with an ad-hoc signature instead: after an install of such a build, macOS asks for the permissions again.
- Windows: run `dist/Moulin Setup 1.0.0.exe` (built with `npm run dist:win`). The installer has no signature, so SmartScreen warns on the first run: click "More info", then "Run anyway". Moulin shows in the notification area. Windows can hide it behind the arrow (^).
- Linux: run `chmod +x Moulin-1.0.0.AppImage`, then start it (built with `npm run dist:linux`). The AppImage needs `libfuse2`. If it does not start on Ubuntu 24.04 (AppArmor blocks the Chromium sandbox), start it with `--no-sandbox`. The icon shows in the top bar on KDE and Ubuntu. On other GNOME desktops, install the "AppIndicator and KStatusNotifierItem Support" extension. To paste, install `xdotool` (X11), or `wtype` or `ydotool` (Wayland).

### Linux with Wayland

Wayland lets an app use a global shortcut only through the desktop portal (KDE, GNOME 48 or later, Hyprland). On other desktops, or if the shortcut does not work:

1. In the system settings (GNOME: Keyboard > Custom Shortcuts, KDE: Shortcuts > Add New > Command), add a shortcut that runs `/path/to/Moulin-1.0.0.AppImage --toggle`.
2. In the Moulin settings, click the shortcut and push Backspace, so that Moulin has no shortcut of its own.

On Wayland, the desktop can also put the pop-up in another place than the bottom of the screen, and GNOME can refuse the clipboard to a background app. Most Linux terminals paste with Ctrl+Shift+V, so the automatic Ctrl+V does not paste there.

## Use

On the first launch, Moulin opens its settings window and asks for permissions:

1. Sign in to ChatGPT in the Moulin window (one time).
2. macOS: allow the microphone.
3. macOS: allow Moulin in System Settings > Privacy & Security > Accessibility, so that it can paste. At the first paste, macOS also asks to let Moulin control System Events.

Then:

- Push the shortcut (F5) to start and to stop a dictation. Push Esc during a dictation to cancel it. A click on the pop-up also stops the dictation.
- `Moulin --toggle` also starts or stops a dictation, so a system shortcut can run it. If Moulin does not run yet, this command starts Moulin and the dictation. In the settings, Backspace in the shortcut field removes Moulin's own shortcut.
- The menu bar icon shows the state: a windmill (ready), a red windmill (recording), turning sails (transcribing), or an amber windmill (problem).
- The menu has the last five texts of the session ("Recent dictations"). If a transcription fails, "Send the last recording again" sends the same audio again.
- Settings: the ChatGPT account (sign in or sign out), the shortcut, paste at the cursor, the microphone, start at login, and the pop-up.

If F5 starts the Dictation of macOS (the microphone key on Apple keyboards), push fn+F5, or turn on "Use F1, F2, etc. keys as standard function keys" in System Settings > Keyboard. If another app uses F5 (for example OpenWhispr), give that app a different key. If a key remapper changes the key (for example a Karabiner-Elements rule that turns F5 into F13), open Settings, click the shortcut, and push the key: Moulin keeps the key that it gets.

On macOS 26, System Settings > Menu Bar > "Allow in the Menu Bar" controls the icon. If the icon does not show, turn on Moulin in that list. Start Moulin from Finder, Spotlight, or `open`. When a terminal starts the binary `Moulin.app/Contents/MacOS/Moulin`, macOS puts the icon in the group of the terminal app and keeps it there. The System Settings toggle cannot remove this link ([CodexBar issue 1440](https://github.com/steipete/CodexBar/issues/1440) gives the repair of `trackedApplications`). Without the icon, start Moulin again to open its settings. The shortcut works also when the icon is hidden.

A dictation stops after 5 minutes, because longer uploads are not tested.

## How it works

1. Hidden browser: Moulin keeps one hidden `BrowserWindow` in the partition `persist:chatgpt`. This is the Chromium engine inside Electron (Chromium 156), not Chrome, Brave, or your default browser. The session sends the user agent of Chrome for the platform. After the sign-in, the window shows `https://chatgpt.com/robots.txt`, not the ChatGPT app. The upload needs only the chatgpt.com origin and the cookies.
2. Audio: the pop-up page records the microphone with `MediaRecorder` (webm/opus, mono, 32 kbit/s). Only the pages of Moulin get the microphone. Moulin keeps the audio until the text is in the clipboard.
3. Upload: at the stop, the main process gives the audio to the chatgpt.com page with `executeJavaScriptInIsolatedWorld`. The code in the isolated world reads the access token from `GET /api/auth/session` and sends `POST /backend-api/transcribe` with `fetch`. The form fields and the headers are the same as the request of the dictation button. The token stays in the page.
4. Completion: the resolved `fetch` is the completion signal. Moulin writes the `text` field to the clipboard, then sends Cmd+V (macOS) or Ctrl+V (Windows, Linux). Moulin never reads the message box of ChatGPT.

Code: [src/main.js](src/main.js) (tray, shortcut, flow, settings), [src/chatgpt.js](src/chatgpt.js) (hidden window, sign-in, upload), [src/overlay/](src/overlay/) (pop-up and recorder), [src/settings/](src/settings/) (settings window), [src/paste.js](src/paste.js) (paste on each platform), [docs/](docs/) (website, and the shapes of the mark).

Data: Moulin keeps `settings.json` and the ChatGPT session (`Partitions/chatgpt`) in `~/Library/Application Support/Moulin` (macOS), `%APPDATA%\Moulin` (Windows), or `~/.config/Moulin` (Linux). The cookies are not encrypted. Only your user account can read the files.

The packaged app turns off the Electron fuses `RunAsNode`, `NODE_OPTIONS`, and the inspect arguments, so that other programs cannot run the Moulin binary as Node with its microphone and Accessibility permissions.

## Development

| Command | What it does |
| --- | --- |
| `npm start` | Runs Moulin from the sources. Quit the installed Moulin first: both use the same profile. |
| `npm run e2e` | The E2E test in the installed (or built) app. It quits a running Moulin and starts it again after. |
| `npm run e2e:dev` | The same test from the sources. |
| `npm run dist:mac`, `dist:win`, `dist:linux` | Builds the installers in `dist/`. |
| `npm run reinstall` | macOS: builds the app, signs it with the local certificate, replaces `~/Applications/Moulin.app`, and starts it. If the signature identity changed, it removes the old permissions, so that macOS asks again with clean entries. |
| `npm run signing:setup` | macOS, one time: creates the local signing certificate in `~/Library/Keychains/moulin-signing.keychain-db`. The login keychain keeps only the password of that keychain. |
| `npm run capture` | Shows the ChatGPT window. Do one dictation with the button of the page. It writes `artifacts/capture-<time>.json` with names and shapes only. |
| `npm run icons` | Makes all the images from the shapes in [docs/mill.js](docs/mill.js): the app icon, the tray images, the header of the settings window, and the images of the website and this README. |
| `npm run sample` | Makes `fixtures/fr-sample.wav` (48 kHz, mono, 16-bit) from `fixtures/fr-sample.txt` with `say` and ffmpeg. |

The E2E test does one full dictation: start, stop, upload, clipboard. A second Moulin process with `--toggle` starts and stops the dictation, the same path as a system shortcut. On macOS, `npm run e2e` starts the app with `open`, so that macOS does not put the menu bar icon in the group of the terminal app. The microphone is the sample WAV (Chromium fake device, `%noloop`). The test never pastes. It writes `artifacts/e2e-<time>.json` and the recording `artifacts/e2e-<time>.webm`. It checks the HTTP status, the word error rate (maximum 0.25), the clipboard, the tray states, the pop-up states, and the silence after the end of the sample. It needs a signed-in session and ffmpeg, and it replaces the clipboard content. The test modes turn off the sandbox of the Chromium audio service (`--disable-features=AudioServiceSandbox`), because that sandbox blocks the read of the WAV file. The normal app keeps the sandbox.

## Website and images

The folder [docs/](docs/) is the website: one page, with no build step and no request to another site. The fonts are in [docs/fonts/](docs/fonts/) (Spectral and EB Garamond, SIL Open Font License).

To publish it with GitHub Pages:

1. Push the repository to GitHub.
2. In Settings > Pages, set "Deploy from a branch", the branch `main`, and the folder `/docs`.
3. In Settings > General > Social preview, upload [docs/media/social.png](docs/media/social.png).

The page is then at https://brohoya.github.io/moulin/. Any static host can serve the folder `docs` without a change. To see the page on this computer, run `python3 -m http.server -d docs 8000` and open http://localhost:8000. A page that opens from the disk (`file://`) cannot load its scripts.

The mark is a windmill whose sails hold a sound. The seven bars in each sail are the loudness of the word "moulin", spoken by the macOS voice Thomas, in seven equal slices from the tip of the sail to the hub. The red hub is the recording light. The wordmark is Spectral Italic, as outlines in [docs/wordmark.js](docs/wordmark.js). Run `npm run icons` after a change of [docs/mill.js](docs/mill.js) or [scripts/media.mjs](scripts/media.mjs).

On the website, the sails turn in a light wind, and a drag with the mouse turns them. "Turn the sails with your voice" asks for the microphone: the page measures only the loudness, and no audio leaves the browser.

## Results (October 2026)

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

Moulin sends the same constant values for `originator`, `x-openai-web-frontend`, and `x-openai-codex-window-type`. It does not send `x-openai-rate-limit-join-workspace`, because its value is unknown.

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

## Terms of Use and risk

- The OpenAI Terms of Use (Europe version, updated January 16, 2026) list under "What you cannot do": "Automatically or programmatically extracting data or Output". Moulin does this, so it breaks the terms, also for personal use.
- The terms also forbid "circumventing any rate limits or restrictions or bypassing any protective measures". Moulin bypasses nothing: each request comes from the real page, one request for each dictation, with a maximum of one automatic retry.
- Known bans (2023 to 2026) involve proxies that got around Cloudflare, accounts shared by many users, or resale. No ban report was found for the dictation tools that use the same endpoint (VoiceTypeX, nub236/dictate, gpt-transcription).
- The risk for one person with a normal volume is low, but it is not zero. OpenAI can suspend the account, and its automatic checks sometimes suspend accounts by mistake. The most probable problem is technical: a change of the web app can stop Moulin.
- Option with no terms risk: the ChatGPT desktop app (with Codex) has a global dictation shortcut on macOS and Windows.

## What is open

- The Windows and Linux builds are not tested. The Wayland behavior (portal, pop-up place, clipboard, paste tools) comes from the documentation of each desktop, not from a test.
- On Windows, the paste starts PowerShell for each dictation (about half a second), and it cannot paste into apps that run as administrator.
- The E2E test uses the fake microphone. The real microphone and the paste are not tested automatically.
- Push-to-talk (hold the key) is not possible with the Electron shortcut API, which has no key-up event.
- The local signing key works only on this Mac. A program that runs as you can use it to sign an app that macOS treats as Moulin, with its permissions. To give Moulin to other people, sign it with an Apple Developer ID certificate and notarize it: that is the standard way to keep the permissions across updates on every Mac.
- The usage errors, the expired session, and the fallback to the full page after a Cloudflare 403 are not tested against the server.
- `sec-ch-ua` has no "Google Chrome" brand, so it is different from a real Chrome. Cloudflare did not challenge it.
