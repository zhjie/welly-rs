# Welly-rs

Welly-rs is a native Rust BBS client focused on making `bbs.newsmth.net` feel
comfortable in a Welly-like terminal experience.

The current goal is not to become a general-purpose terminal emulator. The
project stays focused on a single-window, single-site client until the core
daily-use path is stable.

## Motivation

This project is motivated by [Welly](https://github.com/ytang/welly): its
terminal behavior, keyboard habits, visual proportions, and long-standing fit
for Chinese BBS use. The goal is to preserve that experience while making it
available consistently across macOS, Windows, and Linux. Thanks to Welly and its
contributors for setting the shape of the experience this project is trying to
preserve.

## Usage

Configure your SSH login in `~/.ssh/config`:

```sshconfig
Host bbs.newsmth.net
    User your_username
    IdentityFile ~/.ssh/id_ed25519
    IdentitiesOnly yes
```

Then run:

```sh
cargo run
```

If no username is configured, a login panel will appear on startup.

### Mouse

- **Mouse wheel** sends Welly-style arrow keys to the BBS.
- **Click visible entry text** to move the BBS cursor to that row and press Enter.
- **Click the left edge** to go back.
- **Click the right-side upper/lower screen areas** for PageUp/PageDown.
- **Select text** with click-and-drag; **double-click** a URL-like selection to open it.
- HTTP(S) URLs on the terminal screen are also clickable directly.

### Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| Arrow keys | Navigate |
| Alt + Arrow keys | Welly navigation (Alt-Up/Down = PageUp/PageDown, Alt-Left/Right = Home/End) |
| Cmd + R | Reconnect |
| Cmd + C | Copy selection |
| Cmd + Plus / Minus / 0 | Zoom in / out / reset |
| Ctrl + K | Send raw Ctrl+K to BBS editor |

Command shortcuts are handled locally and not sent to the BBS.

## Current Status

- Native GUI built with `eframe`/`egui`.
- SSH connection to `bbs.newsmth.net`.
- GB18030 decoding for Chinese BBS text.
- ANSI/CSI parsing for common terminal control sequences.
- Welly-like 80×24 layout, font metrics, colors, reverse video, and VT100 art.
- Welly-style keyboard shortcuts for basic navigation.
- Chinese IME input, mouse selection/copy, basic Welly-style mouse navigation,
  anti-idle keepalive, clickable URLs, and image attachment opening.

## Building

Build a local macOS app bundle:

```sh
scripts/build-macos-app.sh
open target/release/bundle/macos/Welly-rs.app
```

Build a local macOS DMG:

```sh
scripts/build-macos-dmg.sh
open target/release/bundle/macos/Welly-rs.dmg
```

Build a local Windows zip package from PowerShell:

```powershell
.\scripts\build-windows-zip.ps1
```

The zip is written to `target\release\bundle\windows\Welly-rs-windows.zip`.
Pass `-Target x86_64-pc-windows-msvc` when building for an explicit Windows
Rust target.

## License

Welly-rs is licensed under the GNU General Public License, version 3 or later.
See [LICENSE](LICENSE).
