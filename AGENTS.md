# Repository Guidelines

## Project Goal

`welly-rs` is a native Rust/egui SSH client for `bbs.newsmth.net` that aims to feel comfortable in a Welly-like terminal experience.

**Phase 1 (backend extraction / main.rs slimming) completed 2026-05-16.** The codebase is now split into a UI-neutral `backend/` and an egui-specific `ui/egui/` frontend, with a `Backend` API surface ready for future alternative frontends (e.g. gpui).

Ongoing work centers on terminal fidelity: GB18030 streaming decode, ANSI/VT100 parser correctness, double-width cell behavior, Welly-like font metrics, keyboard navigation, SSH channel lifecycle, and faithful rendering of BBS screens.

## Project Structure

This is a single Rust binary crate built with `eframe`/`egui`, `tokio`, and `russh`. The source is organized into a UI-neutral backend and an egui frontend:

```
src/
  main.rs          # Entry point: window setup, eframe launch (≤200 lines)
  app.rs           # App struct and eframe::App impl; event-loop glue
  config.rs        # SSH config reading (~/.ssh/config, defaults)
  backend/
    mod.rs         # Re-exports; Backend struct declaration
    api.rs         # Backend public API: new / with_snapshot / send_input / reconnect / shutdown
    cell.rs        # Cell and Color types; UI-neutral rgb() conversion
    terminal.rs    # 24×80 terminal grid; cursor, scroll, attributes, double-width invariants
    ansi_parser.rs # GB18030 streaming decode + ANSI/CSI/VT100 sequence handling
    attachment.rs  # NewSMTH image attachment link detection
    ssh.rs         # SSH connection, auth, channel, anti-idle (russh + tokio)
    snapshot.rs    # TerminalSnapshot<'a>: read-only view for frontends
    input.rs       # UI-neutral InputEvent / KeyEvent / MouseEvent types
    keys.rs        # Welly-style key → SSH byte mapping (Alt-Up → \x1b[5~ etc.)
    mouse.rs       # Welly-style mouse → SSH byte helpers
  ui/
    mod.rs
    egui/
      mod.rs       # egui frontend module root
      fonts.rs     # Font candidate resolution, configure_fonts, cell metrics
      input.rs     # egui::Event → InputEvent translation
      render.rs    # Cell/box-art/cursor/status bar rendering via TerminalSnapshot
      selection.rs # Text selection, copy, double-click URL detection
```

**Backend API surface** (`src/backend/api.rs`):
- `Backend::new(config, notify)` — builds SSH + terminal, starts async worker
- `Backend::with_snapshot(f)` — lock-then-read pattern; `f` gets `&TerminalSnapshot`
- `Backend::send_input(event: InputEvent)` — encodes and sends to SSH
- `Backend::reconnect()` / `Backend::shutdown()`

Frontends produce `InputEvent` values and read state only through `TerminalSnapshot`. The egui frontend is in `src/ui/egui/`; a future gpui frontend would add `src/ui/gpui/`.

## Build, Test, and Development Commands

- `cargo build` compiles the application in debug mode.
- `cargo run` launches the native GUI client locally.
- `cargo test` runs the unit test suite.
- `cargo fmt --check` verifies Rust formatting without changing files.
- `cargo fmt` formats Rust files.
- `cargo clippy --all-targets --all-features -- -D warnings` runs lint checks and treats warnings as failures.

Run `cargo fmt --check`, `cargo test`, and `cargo clippy --all-targets --all-features -- -D warnings` before considering code complete. Use `cargo build` as an additional smoke check for GUI or SSH changes.

## Connection Behavior

The default target is `bbs.newsmth.net:22`. `ConnectionSettings::load_default()` reads a matching `Host bbs.newsmth.net` block from `~/.ssh/config`, currently honoring `Port`, `User`, and `IdentityFile`. If no username is configured, the app shows the login panel. Passwords are only supplied from the runtime login UI.

The app auto-connects on startup when a username is available. Keyboard input is sent directly to SSH. Cmd+R reconnects. Anti-idle sends NUL bytes after the Welly idle window and should stop quietly after the SSH channel ends. Expected channel shutdown should not be logged as an error.

## Coding Style

Use Rust 2021 style and `rustfmt` defaults: 4-space indentation, `snake_case` functions/modules, `PascalCase` types, and `SCREAMING_SNAKE_CASE` constants.

Keep modules aligned with existing responsibilities: ANSI parsing in `backend/ansi_parser`, screen state in `backend/terminal`, SSH transport in `backend/ssh`, rendering in `ui/egui/render`, input translation in `ui/egui/input`, font setup in `ui/egui/fonts`, App event loop in `app.rs`. Avoid broad refactors while changing terminal parsing or rendering behavior.

Prefer explicit error logging at async or thread boundaries, but do not log expected channel shutdown as an error. Treat macOS IMK messages such as `IMKCFRunLoopWakeUpReliable` as system/input-method noise unless there is a matching user-visible input bug.

## Terminal Model Notes

The terminal is a fixed 24x80 grid. `Cell.width` tracks single-width, double-width, and continuation cells:

- Main double-width cell stores the visible character and `width = 2`.
- Continuation cell stores `ch = '\0'` and `width = 0`.
- Rendering and selection skip continuation cells.
- Any write, erase, delete, line clear, or screen clear that touches half of a double-width character must clear or preserve the whole character consistently.

Prioritize unit tests for terminal invariants because they can reproduce rendering bugs without opening the GUI or network. Recent stale-character bugs came from treating half of a double-width character as independently writable.

## Parsing Notes

`ansi_parser` uses streaming `encoding_rs::GB18030` decoding. Keep GB18030 input as byte slices in tests. The parser handles cursor movement, clearing, colors, attributes, scrolling, LF/NEL behavior, and VT100 special graphics.

VT100 `ESC ( 0` special graphics mapping is important for Welly screens. Common mappings include `j/k/l/m/n/q/t/u/v/w/x` to box drawing characters and symbols such as `◆`, `▒`, `°`, `±`, `π`, `≠`, `£`, and `·`.

## Rendering Notes

The UI uses Welly-like terminal metrics:

- `CELL_WIDTH = 18.0`
- `CELL_HEIGHT = 35.0`
- `TERMINAL_COLS = 80`
- `TERMINAL_ROWS = 24`

Configured fonts:

- English: resolved by `fontdb` from the shared candidate list, preferring `Monaco`, then mono-style Cascadia/Caskaydia fonts.
- Chinese: resolved by `fontdb` from the shared candidate list, preferring `Heiti SC`, then mono-style SimHei/Noto/Sarasa fallbacks.

Named egui font families are bound for both fonts with cross-fallback. Text is positioned from the cell top margin so mixed Chinese/ASCII status text such as `在线/最高:1524/9994` has stable top spacing.

Many Welly line/block characters are drawn with egui primitives instead of font glyphs to better match Welly's thicker box art. If line art changes, compare screenshots and tune geometry in `src/ui/egui/render.rs` rather than changing parser mappings first.

### Font Vertical Positioning

The terminal uses an 80×24 cell grid with `CELL_WIDTH = 18.0` and `CELL_HEIGHT = 35.0` logical pixels. English and Chinese characters use separate fonts and sizing.

**Optimization: cap-top alignment.** Goal: minimize the vertical gap between Chinese character tops and English capital tops in mixed rows (e.g. `Rust编程语言`).

Measured at runtime via `ab_glyph` (values at `ENGLISH_FONT_SIZE = 28 px`):
- English ascent (`a_en`): Monaco 22.40, Consolas 20.79
- English cap height (`κ_en`): Monaco 17.00, Consolas 18.00
- Chinese ascent (`a_cn`): Heiti SC 27.52 at 32 px
- Chinese glyph height (`h_cn`): Heiti SC ≈ 26 px

Derived positions (from cell top):
- `T_cn = CHINESE_TOP_MARGIN + (a_cn − h_cn)` ≈ `CHINESE_TOP_MARGIN + 1.5 px`
- `y_off = max((H − r)/2 + κ_en − a_en, b)`
- `T_en = y_off + a_en − κ_en = (H − r)/2` (when not clamped)
- Gap `G = T_en − T_cn`

Optimal anchor `r` that brings Monaco just to the floor clamp `b`:
```
r_opt = H − 2·(b + a_en − κ_en)
      = 35 − 2·(2.0 + 22.4 − 17.0)
      = 35 − 14.8 = 20.2  →  ENGLISH_CAP_HEIGHT_REFERENCE = 20.0
```

Result at `r = 20.0`, `t = 2.0`, `b = 2.0`:
- `T_en` (Monaco, Consolas) = 7.5 px
- `T_cn` (Heiti SC 32 px) ≈ 3.5 px
- Gap `G` ≈ 4.0 px (reduced from 7.5 px at `r = 16`)

The remaining ~4 px gap is irreducible: Monaco's 5.4 px overhead above capitals (`ascent − cap_height`, reserved for diacritics like Á, É) is a fixed property of the font's metric design. Cap-top perfect alignment would require a negative `y_offset`, clipping the English bounding box above the cell.

Chinese glyphs are placed at a fixed top margin (`CHINESE_TOP_MARGIN = 2.0`) from the top of the cell. No cap-height measurement is used for CJK text.

## Keyboard Notes

Arrow keys use Welly-compatible escape sequences:

- Up: `\x1b[A`
- Down: `\x1b[B`
- Right: `\x1b[C`
- Left: `\x1b[D`

Alt-arrow shortcuts map to Welly navigation sequences:

- Alt-Up: `\x1b[5~`
- Alt-Down: `\x1b[6~`
- Alt-Right: `\x1b[4~`
- Alt-Left: `\x1b[1~`

Command shortcuts should be handled locally and not sent to BBS.

## Testing Guidelines

Add focused unit tests near the code they exercise using `#[cfg(test)] mod tests`. Prioritize parser and terminal state transitions. Assert final cursor position, cell contents, widths, colors, attributes, and selected text.

For GUI, font, or SSH changes, document manual verification in the PR or final notes, including the command used and the connection path tested. Screenshots are useful for visible UI changes, but there are no committed reference screenshots at the moment.

## Visual Regression Workflow

When checking visual fidelity:

1. Run `cargo run`.
2. Capture fresh screenshots for the current scenario.
3. Compare against Welly or another trusted live/reference capture.
4. Focus on ASCII/VT100 art thickness and alignment, row/column spacing, cursor placement, reverse-video/background spans, mixed Chinese/ASCII top alignment, missing glyphs or unexpected `?`, and double-width overwrite/clearing behavior.

The Welly source reference lives at `/Users/zhjie/workspace/src/welly` when available locally. Useful reference files include:

- `Welly/WLTerminalFeeder.m` for parser and cursor behavior
- `Welly/WLTerminalView.m` for keyboard/rendering behavior
- `Welly/CommonType.h` for key escape constants

## Commit and Pull Request Guidelines

Use concise imperative commit subjects, for example `Add parser tests for erase sequences`.

Pull requests should include a short summary, test results (`cargo test`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`), and screenshots or recordings for visible UI changes. Link related issues when available and call out any networking, authentication, or platform-specific assumptions.

---

## Investigation Notes

> 历史问题调查记录，供后续排查类似症状时参考。这些问题**已经修复**，不是当前活跃 bug。

### 状态栏行消失 heisenbug 调查（已修复，2026-05-17）

**现象**：主选单和帖子正文页面的 row=0（版面/时间信息）和 row=23（用户名/时间/信件状态）有时不显示，时间越长越容易触发，具有 heisenbug 特征（加了诊断代码后反而不出现）。

**根因**：多个 VT100 规范实现偏差叠加导致 `cursor_row` 漂移，进而使 BBS 的相对光标上移（`ESC[nA`）落在错误行，状态栏内容被写到屏幕外。

**BBS 协议分析**：通过原始数据 dump（`src/ssh.rs` 写入 `%TEMP%\welly-rs-raw.bin`，分析脚本 `scripts/analyze_dump.py`）确认：
- newsmth BBS **不使用 DECSTBM** 滚动区域，scroll_top/scroll_bottom 在正常会话中始终为默认值（0/23）。
- BBS 更新 row=0 的典型模式：绝对定位到 row=23（`ESC[24;NH]`）→ 相对上移（`ESC[23A]` 或组合上移）→ `CR` → `ESC[K`（清行）→ 写内容 → `LF`。
- 相对上移（`ESC[nA`）是关键——它依赖终端的 `cursor_row` 与 BBS 预期完全一致；若 `cursor_row` 偏大，上移后落在错误行，状态栏被写到错误位置。

**修复内容**：
1. `DECSTBM`（`ESC[r`）设置滚动区域后，normal mode 光标 home 到 (0,0)，origin mode home 到 scroll_top。
2. `line_feed` 仅在 `cursor_row == scroll_bottom` 时触发 scroll；在 scroll_bottom 以下不 scroll。
3. `put_char` autowrap 移行后同样只在 `cursor_row == scroll_bottom` 时 scroll。
4. `set_scroll_region` 参数无效时重置为全屏，避免非法区域。
5. 新增 DA/DSR/CPR 终端查询响应支持（`ESC[0c`、`ESC[5n`、`ESC[6n`），通过 SSH channel 回写，对齐 Welly 参考实现。

**排查教训**：
- 早期 heisenbug 假说（TCP 分包 + 两包之间渲染导致空行）在规范修复后不再复现，证实为光标漂移的伴生现象。加了磁盘 dump 后写入延迟可能恰好合并了两包，从而掩盖了真正的光标漂移 bug。
- 终端 emulator 的规范偏差会随时间累积放大 BBS 的 `cursor_row` 误差，长时间会话后才会触发症状。
- 原诊断环境变量（`WELLY_RS_RAW_DUMP`、`WELLY_RS_TRACE_DUMP`）代码仍保留，但不再作为常规排查手段使用。

---

## Known Issues

> 当前活跃但暂不修复的问题。此前 `docs/known-bugs.md` 中的内容已合并至此。

### Ctrl+K 删首字符（单行编辑）

**现象**：编辑状态下，若缓冲区只有一行，`Ctrl+K` 会删除第一个字符并跳回行首；多行时行为正常。

**分析**：Welly 发送原始 `Ctrl+K`（`0x0b`）时也会复现该行为，因此这是 newsmth 编辑器/服务端的固有行为，不是 welly-rs 回归。welly-rs 与 Welly 保持一致，直接发 `0x0b`。无可靠 workaround，暂不修复。
