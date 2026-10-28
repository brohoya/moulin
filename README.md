<h1 align="center"><img src="docs/media/banner.svg" alt="Moulin"></h1>

<p align="center">Speech-to-text in any app, with your ChatGPT subscription.<br>
<a href="https://brohoya.github.io/moulin/">Website</a>    <a href="https://github.com/brohoya/moulin/releases/latest">Download</a></p>

<p align="center"><img src="docs/media/demo.svg" alt="A note on a Mac. The user pushes F5, a dark pop-up shows the recording, the user pushes F5 again, and the sentence appears at the cursor."></p>

ChatGPT has the best multilingual speech-to-text models that I have tested, so pay their subscription if you want to use this tool (please OpenAI, don't sue me).

## Install

On macOS and Linux, run this command in a terminal. It installs Moulin and starts it. Run it again to update.

```sh
curl -fsSL https://brohoya.github.io/moulin/install.sh | sh
```

Or download Moulin for your computer:

- [macOS](https://github.com/brohoya/moulin/releases/latest/download/Moulin-mac-arm64.dmg) (Apple silicon). Drag Moulin to Applications. At the first start, macOS blocks Moulin: open System Settings &gt; Privacy &amp; Security, and click Open Anyway.
- [Windows](https://github.com/brohoya/moulin/releases/latest/download/Moulin-windows-x64.exe). If Windows warns about an unknown app, click "More info", then "Run anyway".
- [Linux](https://github.com/brohoya/moulin/releases/latest/download/Moulin-linux-x86_64.AppImage) (x86_64). Make the file executable, then start it.

Moulin is tested on macOS. The Windows and Linux versions are new.

## Use

1. Sign in to ChatGPT in the Moulin window.
2. Allow the microphone. On macOS, also allow Moulin in System Settings &gt; Privacy &amp; Security &gt; Accessibility, so that it can paste.
3. Push F5 and speak. Push F5 again: the text appears at the cursor, and it stays in the clipboard.

Push Esc to cancel a dictation. The windmill in the menu bar is red while Moulin records, and its sails turn while ChatGPT writes the text. Its menu keeps your last five texts and opens the settings, where you can change the key.

## Help

- If you want to make the  native dictation button F5 useful for the first time in history (because it opens the macOS dictation even after disabling it in settings):
  1. Install [Karabiner-Elements](https://karabiner-elements.pqrs.org)
  2. Add the Moulin rule, which changes this key into F13:
     ```sh
     brew install --cask karabiner-elements
     open "karabiner://karabiner/assets/complex_modifications/import?url=https%3A%2F%2Fbrohoya.github.io%2Fmoulin%2Fkarabiner.json"
     ```
  
     In Karabiner-Elements, click Import, then Enable. Then click the shortcut in the Moulin settings and push the key: the settings show F13.
- Linux: the AppImage needs `libfuse2`. On Ubuntu 24.04, start it with `--no-sandbox`. To paste, install `xdotool` (X11), or `wtype` or `ydotool` (Wayland). On a GNOME desktop other than Ubuntu, install the "AppIndicator and KStatusNotifierItem Support" extension to see the windmill.

## Terms of Use and risk

- The OpenAI Terms of Use (Europe version, updated January 16, 2026) list under "What you cannot do": "Automatically or programmatically extracting data or Output". Moulin does this, so it breaks the terms, also for personal use.
- The terms also forbid "circumventing any rate limits or restrictions or bypassing any protective measures". Moulin bypasses nothing: each request comes from the real page, one request for each dictation, with a maximum of one automatic retry.

---

For developers: [docs/development.md](docs/development.md).