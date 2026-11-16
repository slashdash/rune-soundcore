# rune-soundcore

macOS-only desktop app that configures Soundcore earbuds over Bluetooth. Rune UI (wgpu, native
window, no webview) on top of `openscq30-lib`. Single crate; unit tests live in `#[cfg(test)]`
modules beside the code.

## Commands

- `just check` — `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, then
  `cargo test`. This is the whole gate; run it before reporting a change done.
- `cargo test <name>` runs one test. There are no integration tests or hardware fixtures, so the
  Bluetooth and probe paths are verified by hand with `just probe`.
- `cargo run` — needs real earbuds paired in System Settings, plus Bluetooth permission.
- `just probe` — connect to a device, print every setting to stderr, write
  `~/Library/Application Support/rune-soundcore/snapshot.json` (mode `0600`; it holds the device
  serial number), quit. Confirms the protocol path works.
- `just snap` — headless render of that snapshot to `/tmp/rune-soundcore.png`. Run `just probe`
  first, or point `RUNE_SOUNDCORE_SNAPSHOT` at a snapshot; with no file the page shows a load
  error.
- `RUNE_SOUNDCORE_PAGE=<category>` picks the page for a headless render, using the camelCase
  `CategoryId` (`soundModes`, `equalizer`, `buttonConfiguration`, …, see the snapshot JSON).
- `RUNE_SOUNDCORE_PROBE_WRITE=1 just probe` toggles `TouchTone` on the real device and restores it.
  `RUNE_SOUNDCORE_DUMP=<path>` overrides the snapshot path.
- `just bundle` — release build, copy into `dist/Soundcore.app` using `bundle/Info.plist`, ad-hoc
  `codesign`. `target/` and `dist/` are gitignored.
- `just icon` — redraw `bundle/icon.png` with ImageMagick and rebuild `bundle/AppIcon.icns`; the
  bundle recipe copies that into the app's `Resources`.

## Architecture

- `main` builds the Rune `App`, then `bt::spawn` starts one worker thread that owns the tokio
  runtime and the `OpenSCQ30Session`. Nothing Bluetooth-related runs on the UI thread.
- UI → worker is an unbounded channel of `bt::Cmd` (`AppState::send`); worker → UI is
  `handle.update(|state, _| …)`. A new device operation is a new `Cmd` variant plus its arm in
  `bt::run`.
- The worker pushes whole `Snapshot`s (`state.apply_snapshot`) on connect, after every write, and
  when the device pushes changes; nothing is incremental. Any extra UI-only state belongs in
  `AppState`, not `Snapshot`.
- `--probe` is parsed by this crate; `--snap`, `--crop`, `--scale` are Rune's own headless flags
  handled by `App`, so don't add argument parsing for them.

## Rune UI conventions

- Immediate mode: `col()` / `row()` / `el()` / `text()` builders re-run every render. Every element
  needs a unique stable `.key(...)`; select/overlay keys are reused to track open rows.
- State changes go through closures (`|s: &mut AppState| s.send(Cmd::…)`); there is no event queue.
- `Select` builds into the `layer` passed to `rune_kit::root(body, layer)` in `ui::view`. Push
  overlays into that vector or they won't paint above the content.
- Sliders never write while dragging: they stash into `state.draft` (equalizer: `state.eq_draft`)
  and send `Cmd::Set` only on `Phase::Up`, so device snapshots don't fight the pointer. Follow the
  same pattern for new sliders.
- Accessibility is manual and deliberate: the row carries `.label(...)` / `.role(...)` /
  `.checked(...)`, the visible `text()` is `.a11y_hidden()`. Keep this pairing.
- All user-visible setting/category/model text comes from the protocol crate via `Translate`
  (`id.translate()`, `category.translate()`). `openscq30_lib::i18n::init(&[])` must run before any
  translation (already in `main`).

## Dependency pinning

- `rune` and `rune-kit` must use the same rev, and the four `[patch.crates-io]` crates
  (`wgpu-hal`, `winit`, `parley_engine`, `accesskit_windows`) must use that rev too. Bumping Rune
  means updating all of them; the patch block is repeated here because a dependency's `[patch]`
  does not apply to this workspace.
- `openscq30-lib` and `openscq30-i18n` must stay on the same rev, and it is the
  `SeanathanVT/OpenSCQ30` fork (PR #372), not upstream: upstream has no macOS IOBluetooth RFCOMM
  backend, so pointing these back at Oppzippy's repo breaks connecting.
- GPL-3.0-or-later, and `openscq30-lib` is GPL; keep new dependencies licence-compatible.

## Runtime behaviour worth knowing

- "Scan" lists earbuds the Mac has **already** paired, per model; it is not discovery. Pairing is
  done in System Settings. A missing device usually means a model mismatch or no OS pairing.
- Pairings and settings persist in
  `~/Library/Application Support/rune-soundcore/soundcore.sqlite3`, outside the repo. Stale state
  between runs (auto-reconnect on launch, remembered model) comes from there; delete it to reset.
- `README.md` is the product/UX reference and is current; update it when adding a page or a new
  `Setting` renderer.
- The project is attributed to "the rune-soundcore authors" (`NOTICE`, `Cargo.toml`, bundle
  copyright). Keep it anonymous: never add a real name, handle, home-directory path, email address
  or other PII.
- Commits use the repo-local identity from `just git-setup` (`slashdash` and the GitHub noreply
  address). `.githooks/pre-commit` rejects any author address that is not `@users.noreply.github.com`;
  a fresh clone needs `just git-setup` before its first commit. Never set `user.email` to a personal
  or company address here, and never put one in a tracked file.
