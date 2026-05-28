# email-rs Native Desktop Bug Fix Log

Tracks the three rounds of bug reports and fixes applied to the
`email-desktop-fluent` GPUI app.

---

## Round 1 — Initial bug reports (session 2026-05-27)

| # | Symptom | Root cause | Fix |
|---|---------|-----------|-----|
| a | Select Compose → Discard → nothing happens | `sync_compose_inputs` called `set_tokens` (fires `on_change → update_compose_recipients → cx.notify()`) during a render-triggered update — re-entrant state mutation; **also** SuggestionPopover `on_mouse_down_out` called `cx.stop_propagation()` which swallowed the Discard click | Added `set_tokens_silent()` to `TokenInput` (skips `on_change`); used it in `sync_compose_inputs`; removed `stop_propagation` from `on_mouse_down_out` (Round 2) |
| b | Autocomplete suggestions overlap | Multiple `TokenInput` instances (To/CC/BCC) each rendered a `SuggestionPopover` with the same hardcoded element ID `"suggestion-popover"` — GPUI merged them | Assigned unique `ElementId` via `motion_key = entity_id` for both the list div and each item row |
| c | BCC button does nothing | Same `stop_propagation` issue as (a); also `TokenInput` had no `on_focus_out` handler so the popup never closed when focus left | Added `on_focus_out` subscription in `TokenInput.render()` that calls `close_popup()`; `stop_propagation` removal in Round 2 |
| d | No option to render external images | `RichDocumentBlock::Image` was always rendered as literal text `"Image: alt (src)"` in `DocumentViewer`; no `show_external_images` state existed | Added `show_external_images: bool` to `NativeAppState`; `strip_external_images()` removes external Image blocks when false; banner with "Show images" button displayed when blocked images are detected; actual image rendering fixed in Round 2 |
| e | Cannot select/copy text | GPUI div-based rendering has no native text selection | Added "Copy" button to reading pane toolbar that writes body plain-text to clipboard via `app_cx.write_to_clipboard()` |
| f | App hangs after rapid email clicking | 26 occurrences of `tokio::runtime::Runtime::new()` (4-thread pools each) in every background op exhausted OS thread limits under rapid selection | Replaced all background runtimes with `Builder::new_current_thread().enable_all().build()` |

**Files changed (Round 1):**
- `crates/email-desktop-fluent/src/main.rs`
- `crates/email-client/src/lib.rs`
- `crates/email-server/src/api/messages.rs`
- `crates/email-server/src/api/mod.rs`
- `crates/email-types/src/lib.rs`
- `FluentGUI/crates/fluent-primitives/src/token_input.rs`
- `FluentGUI/crates/fluent-primitives/src/suggestion_popover.rs`

---

## Round 2 — Follow-up bug reports (session 2026-05-27)

| # | Symptom | Root cause | Fix |
|---|---------|-----------|-----|
| 1 | "Show Images" button changes boxes to `"Image: https..."` instead of rendering | Round 1 fix only stripped images when `show_external_images=false`; the `DocumentViewer` still rendered `Image` blocks as text regardless | `DocumentViewer.render_block()` now uses `gpui::img(src)` for `http://`, `https://`, and `data:` URLs; images load async via GPUI's built-in image cache; constrained to `max_w_full` + `max_h(480px)` |
| 2 | Copy still doesn't work; compose body needs text editor copy/paste | Copy button in toolbar requires knowing it exists; Textarea had no clipboard shortcuts at all | Added `Ctrl+C` / `Ctrl+A` (copy all), `Ctrl+V` (paste at end), `Ctrl+X` (cut) to `Textarea`; added `Ctrl+C` key binding on `NativeReadingPane` key context (`ReadingPaneKeyAction::CopyBody`) that copies body plain-text |
| 3 | Discard button still does nothing | Round 1 fix added `on_focus_out` to close the popup, but `SuggestionPopover.on_mouse_down_out` still called `cx.stop_propagation()` which swallowed Discard's `mouse_down` event **before** focus-out fired | Removed `cx.stop_propagation()` from `SuggestionPopover.on_mouse_down_out`; popup still closes via `on_close` callback, but click now propagates to its target |
| 4 | Autocomplete suggestion items clipped top and bottom | Item rows used `flex_col + justify_center + min_h(option_height)` inside `overflow_y_scroll`; GPUI's flex layout collapsed the item height to `min_h`, clipping multi-line rows | Replaced with `flex_shrink_0` + explicit `py` computed as `(option_height - line_height) / 2` for single-line; two-line rows (label + detail) expand naturally |
| 5 | Settings button does nothing | Same `stop_propagation` issue as (3) — if a TokenInput popup was open anywhere, clicking Settings fired `on_mouse_down_out → stop_propagation` before the click reached Settings | Same fix as (3): removing `stop_propagation` |

**Files changed (Round 2):**
- `FluentGUI/crates/fluent-layout/src/document_viewer.rs`
- `FluentGUI/crates/fluent-primitives/src/suggestion_popover.rs`
- `FluentGUI/crates/fluent-primitives/src/textarea.rs`
- `crates/email-desktop-fluent/src/main.rs`

---

## Known remaining limitations

- **Text selection**: GPUI's GPU-rendered div layout has no native OS text selection. `Ctrl+C` copies the entire body/compose text; arbitrary mouse-drag selection is not implemented. Full selection would require a text-editor widget with layout engine, hit-testing, and selection rendering (out of scope).
- **Inline attachment images** (`cid:` src): Rendered as alt-text label; linking CID references to stored attachment bytes requires a lookup pass through the attachment cache.
- **Compose body cursor**: `Textarea` appends text at the end; there is no visual caret or click-to-position support. This is also a full text editor widget problem.
- **Ctrl+Z undo**: Absorbed (no-op) in Textarea to prevent inserting literal `z`; undo history not implemented.
