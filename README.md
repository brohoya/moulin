<h1 align="center"><img src="docs/media/banner.svg" alt="Moulin"></h1>

<p align="center">Speech-to-text in any app, with your ChatGPT subscription.<br>
<a href="https://brohoya.github.io/moulin/">Website</a> &nbsp; <a href="https://github.com/brohoya/moulin/releases/latest">Download</a></p>

<p align="center"><img src="docs/media/demo.svg" alt="A note on a Mac. The user pushes F5, a dark pop-up shows the recording, the user pushes F5 again, and the sentence appears at the cursor."></p>

In French, a "moulin à paroles" is a person who talks all the time.

## Install

On macOS and Linux, run this command in a terminal. It installs Moulin and starts it. Run it again to update.

```sh
curl -fsSL https://brohoya.github.io/moulin/install.sh | sh
```

Or download Moulin for your computer:

- [macOS](https://github.com/brohoya/moulin/releases/latest/download/Moulin-mac-arm64.dmg) (Apple silicon). Drag Moulin to Applications. At the first start, macOS blocks Moulin: open System Settings > Privacy & Security, and click Open Anyway.
- [Windows](https://github.com/brohoya/moulin/releases/latest/download/Moulin-windows-x64.exe). If Windows warns about an unknown app, click "More info", then "Run anyway".
- [Linux](https://github.com/brohoya/moulin/releases/latest/download/Moulin-linux-x86_64.AppImage) (x86_64). Make the file executable, then start it.

Moulin is tested on macOS. The Windows and Linux versions are new.

## Use

1. Sign in to ChatGPT in the Moulin window.
2. Allow the microphone. On macOS, also allow Moulin in System Settings > Privacy & Security > Accessibility, so that it can paste.
3. Push F5 and speak. Push F5 again: the text appears at the cursor, and it stays in the clipboard.

Push Esc to cancel a dictation. The windmill in the menu bar is red while Moulin records, and its sails turn while ChatGPT writes the text. Its menu keeps your last five texts and opens the settings, where you can change the key.

## Help

- F5 does nothing: on Apple keyboards, F5 can start the dictation of macOS. Push fn+F5, or choose another key in the settings.
- No windmill in the menu bar (macOS 26): turn on Moulin in System Settings > Menu Bar > "Allow in the Menu Bar".
- Windows can hide the windmill behind the arrow (^) of the notification area.
- Linux: the AppImage needs `libfuse2`. On Ubuntu 24.04, start it with `--no-sandbox`. To paste, install `xdotool` (X11), or `wtype` or `ydotool` (Wayland). On a GNOME desktop other than Ubuntu, install the "AppIndicator and KStatusNotifierItem Support" extension to see the windmill.

## Terms of Use and risk

- The OpenAI Terms of Use (Europe version, updated January 16, 2026) list under "What you cannot do": "Automatically or programmatically extracting data or Output". Moulin does this, so it breaks the terms, also for personal use.
- The terms also forbid "circumventing any rate limits or restrictions or bypassing any protective measures". Moulin bypasses nothing: each request comes from the real page, one request for each dictation, with a maximum of one automatic retry.
- Known bans (2023 to 2026) involve proxies that got around Cloudflare, accounts shared by many users, or resale. No ban report was found for the dictation tools that use the same endpoint (VoiceTypeX, nub236/dictate, gpt-transcription).
- The risk for one person with a normal volume is low, but it is not zero. OpenAI can suspend the account, and its automatic checks sometimes suspend accounts by mistake. The most probable problem is technical: a change of the web app can stop Moulin.
- Option with no terms risk: the ChatGPT desktop app (with Codex) has a global dictation shortcut on macOS and Windows.

---

For developers: [docs/development.md](docs/development.md).
