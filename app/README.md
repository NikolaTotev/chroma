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

## Run the desktop app

Requires the Tauri toolchain and the platform webview:

- **Windows:** WebView2 (preinstalled on Windows 11).
- **Linux:** `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`.
- ffmpeg on `PATH` for export.

```sh
cd app
npm install
npm run tauri dev     # builds src-tauri and opens the window
npm run tauri build   # packaged installer
```

> The `src-tauri` crate is intentionally **outside** the core Rust workspace
> (it declares its own `[workspace]`) so the heavy webview dependency tree never
> touches `cargo build`/`cargo test` of the engine crates.
