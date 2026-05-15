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

HTTP(S) URLs on the terminal screen are clickable. Select a URL-like text
without `http://` or `https://`, then double-click the selection to open it with
`https://`.

Mouse wheel scrolling sends Welly-style arrow keys to the BBS. Click visible
entry text to move the BBS cursor to that row and press Enter. Click the left
edge to go back, or the right-side upper/lower screen areas for PageUp/PageDown.

## Current Status

- Native GUI built with `eframe`/`egui`.
- SSH connection to `bbs.newsmth.net`.
- GB18030 decoding for Chinese BBS text.
- ANSI/CSI parsing for common terminal control sequences.
- Welly-like 80x24 layout, font metrics, colors, reverse video, and VT100 art.
- Welly-style keyboard shortcuts for basic navigation.
- Chinese IME input, mouse selection/copy, basic Welly-style mouse navigation,
  anti-idle keepalive, clickable URLs, and image attachment opening.

## Font Vertical Positioning

The terminal uses a 80×24 cell grid with `CELL_WIDTH = 18.0` and `CELL_HEIGHT = 35.0` logical
pixels. English and Chinese characters use separate fonts and sizing.

### Optimization: cap-top alignment

Goal: minimize the vertical gap between Chinese character tops and English capital tops in
mixed rows (e.g. `Rust编程语言`). This is an optimization over two parameters:

```
Variables
  t = CHINESE_TOP_MARGIN          (Chinese bounding-box top from cell top)
  r = ENGLISH_CAP_HEIGHT_REFERENCE (anchor constant in English formula)
  b = ENGLISH_TOP_MARGIN           (floor clamp for English y_offset)

Measured at runtime via ab_glyph (values at ENGLISH_FONT_SIZE = 28 px):
  a_en  = English ascent            Monaco 22.40, Consolas 20.79
  κ_en  = English cap height ('H')  Monaco 17.00, Consolas 18.00
  a_cn  = Chinese ascent            Heiti SC 27.52 at 32 px
  h_cn  = Chinese glyph height      Heiti SC ≈ 26 px ('字' above baseline)

Derived positions (from cell top):
  T_cn  = t + (a_cn − h_cn)              ≈ CHINESE_TOP_MARGIN + 1.5 px
  y_off = max( (H − r)/2 + κ_en − a_en , b )
  T_en  = y_off + a_en − κ_en = (H − r)/2   (when not clamped)
  Gap G = T_en − T_cn

Minimise G: increase r until Monaco just hits the floor clamp b
  r_opt = H − 2·(b + a_en − κ_en)
        = 35 − 2·(2.0 + 22.4 − 17.0)
        = 35 − 14.8 = 20.2  →  ENGLISH_CAP_HEIGHT_REFERENCE = 20.0 (rounded)

Result at r = 20.0, t = 2.0, b = 2.0:
  T_en (Monaco, Consolas) = 7.5 px  (both fonts anchor to same cap-top)
  T_cn (Heiti SC 32 px)   ≈ 3.5 px
  Gap G                   ≈ 4.0 px  (reduced from 7.5 px at r = 16)
```

The remaining ~4 px gap is irreducible: Monaco's 5.4 px overhead above capitals
(`ascent − cap_height`, reserved for diacritics like Á, É) is a fixed property of the
font's metric design. Cap-top perfect alignment would require a negative `y_offset`,
clipping the English bounding box above the cell.

### Chinese (fixed top margin)

Chinese glyphs are placed at a fixed top margin: `y = CHINESE_TOP_MARGIN = 2.0` from the
top of the cell. No cap-height measurement is used for CJK text.

## Roadmap

1. Test cross-platform compatibility.

   Stabilize in this order:

   [x] macOS
   [x] Windows
   [ ] Linux (maybe later)

   Pay special attention to fonts, IME behavior, keyboard modifiers, browser
   opening, and `~/.ssh/config` / SSH config equivalents.

2. Consider a minimal settings UI.

   Keep this optional. The preferred login path is still SSH config plus manual
   login when no config exists.

3. Consider OS-level credential storage only.

   Do not store passwords in plaintext. If password persistence is ever added,
   use the operating system keychain/credential manager.

4. Future Ideas

   Expose a headless Welly-rs backend for alternative frontends, such as an Emacs
   client. The Rust backend should own SSH, decoding, ANSI parsing, and the
   Welly-style screen buffer; frontends should only render that buffer and
   forward input.

## TODO

- Add Welly-style screen button hotspots for labels such as compose, delete,
  help, board modes, reply, and mail actions.
- Add author hotspots and context actions.
- Add IP address tooltips.
- Add precise compose-mode cursor movement by clicking a target cell.
- Add contextual menus for mouse hotspots.

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
