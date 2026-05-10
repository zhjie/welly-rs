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

## Usage (for test)

Configure your SSH login in `~/.ssh/config`:

```sshconfig
Host bbs.newsmth.net
    User cppbuilder
    IdentityFile ~/.ssh/id_ed25519
    IdentitiesOnly yes
```

Then run:

```sh
cargo run
```

## Current Status

- Native GUI built with `eframe`/`egui`.
- SSH connection to `bbs.newsmth.net`.
- GB18030 decoding for Chinese BBS text.
- ANSI/CSI parsing for common terminal control sequences.
- Welly-like 80x24 layout, font metrics, colors, reverse video, and VT100 art.
- Welly-style keyboard shortcuts for basic navigation.
- Chinese IME input, mouse selection/copy, anti-idle keepalive, and image attachment opening.

## Roadmap

1. Test cross-platform compatibility.

   Stabilize in this order:

   [x] macOS
   [x] Windows
   [ ] Linux (maybe later)

   Pay special attention to fonts, IME behavior, keyboard modifiers, browser
   opening, and `~/.ssh/config` / SSH config equivalents.

2. Recognize links and make them clickable.

   Image attachments are already detected from article footers. Plain article
   links and other URLs should share the same hotspot/click path later.

3. Consider a minimal settings UI.

   Keep this optional. The preferred login path is still SSH config plus manual
   login when no config exists.

4. Consider OS-level credential storage only.

   Do not store passwords in plaintext. If password persistence is ever added,
   use the operating system keychain/credential manager.

5. Future Ideas

   Expose a headless Welly-rs backend for alternative frontends, such as an Emacs
   client. The Rust backend should own SSH, decoding, ANSI parsing, and the
   Welly-style screen buffer; frontends should only render that buffer and
   forward input.

## Development

```sh
cargo build
cargo run
cargo test
```

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

## License

Welly-rs is licensed under the GNU General Public License, version 3 or later.
See [LICENSE](LICENSE).
