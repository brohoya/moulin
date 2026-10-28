'use strict';
// Pastes the clipboard at the cursor of the active app: Cmd+V on macOS, Ctrl+V on Windows and Linux.
const { execFile } = require('node:child_process');

function run(command, args) {
  return new Promise((resolve, reject) => {
    execFile(command, args, { timeout: 5000 }, (err) => (err ? reject(err) : resolve()));
  });
}

async function paste() {
  if (process.platform === 'darwin') {
    // Needs the Accessibility permission. macOS also asks one time to let Moulin control System Events.
    return run('osascript', ['-e', 'tell application "System Events" to keystroke "v" using command down']);
  }
  if (process.platform === 'win32') {
    return run('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', "(New-Object -ComObject WScript.Shell).SendKeys('^v')"]);
  }
  // Linux: xdotool on X11. On Wayland, wtype (wlroots desktops) or ydotool (needs its daemon).
  const wayland = process.env.XDG_SESSION_TYPE === 'wayland';
  const tools = [
    ...(wayland ? [['wtype', ['-M', 'ctrl', '-k', 'v', '-m', 'ctrl']], ['ydotool', ['key', '29:1', '47:1', '47:0', '29:0']]] : []),
    ['xdotool', ['key', '--clearmodifiers', 'ctrl+v']],
  ];
  for (const [command, args] of tools) {
    try {
      return await run(command, args);
    } catch (err) {
      if (err.code !== 'ENOENT') throw err;
    }
  }
  throw new Error(wayland ? 'install wtype or ydotool' : 'install xdotool');
}

module.exports = { paste };
