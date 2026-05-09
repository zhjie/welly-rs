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

## Current Status

- Native GUI built with `eframe`/`egui`.
- SSH connection to `bbs.newsmth.net`.
- GB18030 decoding for Chinese BBS text.
- ANSI/CSI parsing for common terminal control sequences.
- Welly-like 80x24 layout, font metrics, colors, reverse video, and VT100 art.
- Welly-style keyboard shortcuts for basic navigation.

## Roadmap

1. Improve connection stability

   The client still disconnects too often. Investigate SSH session lifetime,
   channel read/write loops, keepalive behavior, reconnect handling, and error
   reporting before adding broader configuration features.

2. Add connection and login configuration

   Support a simple single-site configuration: host, port, username, and
   authentication method. Default to `bbs.newsmth.net:22`.

   Avoid multi-user, multi-site, and multi-tab management for now. For password
   storage, prefer platform credential storage where practical, such as macOS
   Keychain; keep plain config files limited to non-secret fields.

3. Detect links and open them in the browser

   Identify URLs in terminal text, show a hover affordance, and open them with
   the system browser on click. Start with ordinary HTTP/HTTPS URLs before
   adding more BBS-specific patterns.

4. Preview image attachments

   Some pages contain image attachments. Build this after link detection so the
   same hit testing and URL handling can be reused. First version can open or
   preview on click; hover preview can come later if it feels reliable.

5. Test cross-platform compatibility

   Stabilize in this order:

   - macOS
   - Windows
   - Linux

   Pay special attention to fonts, keyboard modifiers, browser opening,
   configuration directories, and credential storage differences.

## Future Ideas

- Expose a headless Welly-rs backend for alternative frontends, such as an Emacs
  client. The Rust backend should own SSH, decoding, ANSI parsing, and the
  Welly-style screen buffer; frontends should only render that buffer and forward
  input.

## Development

```sh
cargo build
cargo run
cargo test
```

## License

Welly-rs is licensed under the GNU General Public License, version 3 or later.
See [LICENSE](LICENSE).
