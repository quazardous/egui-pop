---
egui-pop-core: minor
---

The decorated input gains two text fields (experimental, as the input). `Input::masked` is a masked field for a password, a token or a key: every character a bullet, nothing to copy or cut, and its value never given to assistive technologies (they get its name and the password-input role; the value and text runs are bullets) nor to agents (its kind is `masked`). `Input::multiline` is a multi-line field: `rows` lines tall while its text is short (2 by default), growing with its text up to `max_rows` (8 by default), then scrolling; Enter starts a new line and Ctrl+Enter (Cmd+Enter on macOS) submits, keeping the focus, or with `submit_on_enter(true)` Enter submits and Shift+Enter starts a new line; agents read its kind as `multiline`. The gallery's Inputs page shows both.
