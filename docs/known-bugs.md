# Known Bugs

## 2026-05-11

### Ctrl+K deletes the first character in single-line editing

During editing, `Ctrl+K` behaves differently depending on the edit buffer:

- If the content has only one line, `Ctrl+K` deletes the first character and
  jumps back to the first character.
- If the content has multiple lines, `Ctrl+K` deletes normally.

Welly also reproduces the single-line behavior when sending raw `Ctrl+K`, so
this appears to be a newsmth editor/server-side quirk rather than a welly-rs-only
regression.

No reliable workaround is active. welly-rs sends raw `Ctrl+K` (`0x0b`) like
Welly, preserving the server behavior.
