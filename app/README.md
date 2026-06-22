# Chroma Studio (desktop GUI)

A Tauri + Svelte editor for Chroma. One window does the whole loop — **Record
→ edit → Export**: click Record to capture the screen, stop to drop straight
into editing it (live composited preview, presets/background/scene panel, a
timeline of modifiers, undo/redo), then Export to MP4/GIF — all driven by the
`chroma-studio` engine over Tauri commands.

> **Recording needs a native X11 / Xorg session.** It uses the same X11 backend
> as the CLI `chroma record`; on Wayland (Ubuntu's default) or rootless Xwayland
> the Record button returns "unavailable". Log in via **"Ubuntu on Xorg"** to
> record. Editing and export work on any session — including over a clip opened
> with `Studio::load_clip`. Before you record, the editor runs over a built-in
> synthetic screen so the UI is usable immediately.

## Architecture

```
app/
  index.html, vite.config.ts, svelte.config.js   # Vite + Svelte 5 (runes) SPA
  src/
    main.ts            # mounts the app
    App.svelte         # the whole editor UI
    lib/
      types.ts         # TS mirrors of the Rust serde shapes
      api.ts           # Tauri `invoke` wrappers + an in-browser mock studio
  src-tauri/           # the Tauri shell (its own cargo workspace)
    src/lib.rs         # commands wrapping `chroma-studio::Studio`
    tauri.conf.json    # window + bundle config
    capabilities/      # permission to call the commands
    icons/             # app icons
```

The Rust side is deliberately thin: every command locks a shared `Studio` and
returns a fresh state snapshot. All real logic (edits, undo, preview render,
export) lives in `crates/chroma-studio` and is unit-tested there, so the editor's
behaviour is verifiable without launching a window.

`src/lib/api.ts` detects whether it is running inside Tauri. In a plain browser
(e.g. `npm run dev`/`npm run build`) it falls back to an in-memory mock studio so
the entire UI is usable and the front end builds/type-checks with no backend.

## Develop the front end (browser, no Rust needed)

```sh
cd app
npm install
npm run dev       # http://localhost:1420 — runs against the mock studio
npm run build     # type-safe production build into dist/
npm run check     # svelte-check (0 errors)
```

## Run the desktop app (record → edit → export)

Recording captures the screen through the X11 backend, so on Ubuntu you must be
on a **native Xorg session** (Wayland is the default and the Record button will
report "unavailable" there). Editing and export work on any session.

### 1. Log in on Xorg (for recording)

Log out → on the login screen click your name → click the **⚙ gear** (bottom
right) → choose **"Ubuntu on Xorg"** → log in. Confirm:

```sh
echo $XDG_SESSION_TYPE     # must print: x11   (not "wayland")
```

### 2. Install the toolchain (one-time)

```sh
# Rust + ffmpeg + Node
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
sudo apt update
sudo apt install -y ffmpeg build-essential pkg-config nodejs npm

# Tauri's webview deps (Ubuntu 24.04 names; on 22.04 use libwebkit2gtk-4.0-dev)
sudo apt install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev libssl-dev libxdo-dev curl wget file
```

On **Windows** the webview is WebView2 (preinstalled on Windows 11); ffmpeg must
be on `PATH`. (Recording is X11-only, so it is a no-op on Windows.)

### 3. Launch the window

```sh
cd app
npm install           # first time only
npm run tauri dev     # compiles the Rust shell + opens Chroma Studio
npm run tauri build   # …or a packaged installer/AppImage
```

The first build takes a few minutes. When the window opens, the status badge
reads **"desktop"**.

### 4. In the window

1. **● Record** (top-left) — turns red with a timer; everything on screen is
   captured.
2. **■ Stop** — the take loads as your source; the preview shows your real
   footage with a starter timeline (cursor-follow + cursor marker + a ripple at
   each click).
3. **Edit** — scrub the timeline; pick a preset or set the background/scene;
   add/remove modifiers (Text, Cursor Follow, Highlight, Click Ripple); **Undo /
   Redo** anytime.
4. **Export** — enter a path like `~/demo.mp4` (or `.gif`); the styled result is
   composited and written.

If Record says "unavailable", you are on Wayland — redo step 1.

### Sanity check without recording

The CLI renders a styled demo on any session, to confirm the editor/export half:

```sh
# from the workspace root (the folder with Cargo.toml)
cargo run -p chroma-app --bin chroma -- render ~/demo.mp4 5 30 && xdg-open ~/demo.mp4
```

> The `src-tauri` crate is intentionally **outside** the core Rust workspace
> (it declares its own `[workspace]`) so the heavy webview dependency tree never
> touches `cargo build`/`cargo test` of the engine crates.
