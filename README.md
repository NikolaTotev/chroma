# Chroma

Screen-demo capture & compositing studio for Linux (Ubuntu, X11 first,
Wayland-ready). Records the screen plus a timestamped input-event stream, then
composites it non-destructively over a styled background with an animated
virtual camera, crop/zoom, text, and cursor-follow effects — exporting to MP4
or GIF.

> **The architectural idea:** nothing is baked at capture time. The recorder
> produces a raw screen video plus a separate event log; every effect is a
> non-destructive modifier evaluated at render time. See
> `claude_info/Chroma-Requirements-Architecture.docx` for the full spec and
> `claude_info/ORCHESTRATION.md` for how the multi-agent build is run.

## Status

- **M0 — Contracts:** the three `-api` crates compile with fakes and clean docs.
- **M1 — X11 capture:** `chroma-capture-x11` implements screen frames
  (`GetImage`), input events (XInput2), and a shared monotonic clock.
- **M2 — Render core:** `chroma-compositor` (CPU) composites a background +
  camera-transformed scene inset; `chroma-render` wires the §3.4 pipeline into
  one deterministic composited frame. (Live preview window and the wgpu
  compositor are still to come.)
- **M3 — Export:** `chroma-media-ffmpeg` encodes composited frames to **MP4**
  (libx264) and **GIF** (two-pass palette) via the `ffmpeg` CLI. Requires
  `ffmpeg` on `PATH`.
- **M4 — Effects:** `chroma-modifiers` implements Crop/Zoom, Text,
  Cursor-Follow, and Highlight as `Modifier`s, built from project `ModifierSpec`
  data. `chroma-app` ties it together into the **`chroma` CLI**.
- **M5 — Camera & cursor:** `chroma-camera` adds a critically-damped spring
  (`SpringSmoother`, a `CameraSmoother`) so cursor-follow glides instead of
  snapping (CAM-02), injected into the render core. `chroma-modifiers` gains a
  synthetic cursor marker (CAM-05) and a click ripple (CAM-06); `chroma render`
  now demos all three end to end.
- **M6 — Project & editing:** keyframe `Track`s (core-api) drive a
  `KeyframeCamera` modifier (EDT-06); `chroma-project` adds versioned JSON
  save/load (EDT-11), a lossless undo/redo `History` of `EditCommand`s (EDT-10),
  and built-in look `Preset`s (EDT-09). `chroma-media-ffmpeg` gains a VAAPI
  hardware-encode path (EXP-08).
- **M7 — Wayland capture:** `chroma-capture-wayland` is the Wayland backend slot
  implementing the capture contracts; the ScreenCast-portal + PipeWire flow is
  behind an opt-in `portal` feature (needs a live Wayland session), reporting
  `Unavailable` until then. See `crates/chroma-capture-wayland/src/portal.rs`.
- **GUI — Chroma Studio:** `chroma-studio` is the headless editor engine — it
  **records** the screen to a source clip (`chroma-capture-x11` → temp MP4 +
  event log), **decodes** that footage on demand (`chroma-media-ffmpeg`'s
  `FfmpegDecoder`/`FfmpegFrameReader`), and renders/exports the styled result.
  `app/` is a Tauri + Svelte desktop editor over it: **Record → edit → Export**
  in one window (preview, presets, scene styling, timeline, undo/redo). See
  [app/README.md](app/README.md).

## Running on Ubuntu

Tested on Ubuntu 22.04 / 24.04. All commands run from the **workspace root**
(the folder with this `Cargo.toml` and `app/`).

### Prerequisites

```sh
# Rust — the repo pins its toolchain via rust-toolchain.toml; rustup honors it
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# ffmpeg is required for all video export
sudo apt update
sudo apt install -y ffmpeg build-essential pkg-config
```

### The `chroma` CLI (quickest path to a video)

```sh
# Render the built-in demo (spring cursor-follow + cursor marker + click ripple
# + title) over a styled background. Works on any session:
cargo run -p chroma-app --bin chroma -- render out.mp4 5 30
#                                         ^cmd   ^file ^secs ^fps

# Same, but GIF:
cargo run -p chroma-app --bin chroma -- render out.gif 4 20

# Record the actual desktop → cursor-follow styled video:
cargo run -p chroma-app --bin chroma -- record recording.mp4 8 30
```

Then `xdg-open out.mp4`.

> ⚠️ **`record` needs a native X11 session.** Ubuntu defaults to **Wayland**,
> where X11 cannot grab the root window (you'll get `BadMatch`/`Unavailable` —
> the limitation M7's PipeWire path will solve). To record today, log out and
> choose **"Ubuntu on Xorg"** at the login screen (gear icon), then run it.
> `render` works on either session.

Lower-level demos:

```sh
cargo run -p chroma-render --example compose                  # → out.bmp (one frame)
cargo run -p chroma-media-ffmpeg --example studio -- out.mp4  # → effects clip
```

### Chroma Studio (the desktop GUI)

Install the GUI toolchain — Node plus Tauri's webview dependencies:

```sh
sudo apt install -y nodejs npm   # or use nvm for a newer Node (>= 18)

# Tauri 2 system deps (Ubuntu 24.04 names):
sudo apt install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev \
  libssl-dev libxdo-dev curl wget file
```
On **Ubuntu 22.04** the webkit package is `libwebkit2gtk-4.0-dev` (not `-4.1-dev`).

Run / bundle it:

```sh
cd app
npm install
npm run tauri dev      # builds the Rust shell + opens the window (first build is slow)
npm run tauri build    # → installer/AppImage under app/src-tauri/target/release/bundle/
```

In the window: pick a **preset**, tweak **background/scene**, scrub the
**timeline**, add/remove **modifiers**, **undo/redo**, and **Export** to MP4/GIF.
The status badge reads **"desktop"** when the Rust backend is live.

### Front end only (no Rust/Tauri, just a browser)

Runs the UI against an in-browser mock studio — handy for a quick look:

```sh
cd app
npm install
npm run dev            # http://localhost:1420  (badge shows "preview (mock)")
```

### Notes

- The first `cargo` build of the workspace, and the first `tauri dev`, take a few
  minutes; later runs are incremental.
- **Recording needs a native X11/Xorg session** (see the `record` note above) —
  the Studio's Record button uses the same X11 backend. Until you record, preview
  and export run over a built-in synthetic screen so the UI is usable immediately;
  after recording, they run over your real footage (decoded on demand).
- Wayland *capture* (`chroma-capture-wayland`) is scaffolded but its live PipeWire
  path isn't wired yet; record on an Xorg session for now.

## Workspace layout

```
crates/
  chroma-core-api/     # value types + Modifier/Compositor traits (no logic)
  chroma-capture-api/  # ScreenCapturer, EventSource, Clock + Frame/InputEvent
  chroma-media-api/    # Decoder, Encoder, FrameSource + codec/param types
  chroma-capture-x11/      # X11 capture backend (Linux); stub elsewhere
  chroma-capture-wayland/  # Wayland capture backend (portal+PipeWire; feature-gated)
  chroma-compositor/       # CPU reference compositor (implements Compositor)
  chroma-camera/           # critically-damped spring camera smoother (CameraSmoother)
  chroma-render/           # deterministic §3.4 render pipeline → composited frame
  chroma-media-ffmpeg/     # ffmpeg-subprocess Encoder → MP4 / GIF
  chroma-modifiers/        # effects: CropZoom, Text, CursorFollow, Highlight, KeyframeCamera
  chroma-project/          # versioned save/load, undo/redo history, look presets
  chroma-studio/           # headless editor engine (project + history + preview + export)
  chroma-app/              # composition root: the `chroma` CLI
app/                       # Tauri + Svelte desktop editor (Chroma Studio)
```

Contract crates hold **only** traits, value types, and fakes. Implementation
crates depend on the `-api` crates, never on each other's internals.

The deferred crates have now landed; the live PipeWire capture (Wayland
`portal` feature) is the one remaining piece that needs a real Wayland session
to finish and verify.

## Build

Requires a stable Rust toolchain (pinned via `rust-toolchain.toml`).

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo doc --no-deps
```

`chroma-capture-x11` only builds its real implementation on Linux; on other
platforms it compiles to a stub that reports `Unavailable`. To build/test the
real backend from Windows, use WSL (project on `/mnt/c`, target dir on the Linux
filesystem for speed):

```sh
wsl -d Ubuntu-24.04 -- bash -lc \
  'source ~/.cargo/env; cd /mnt/c/.../chroma; \
   CARGO_TARGET_DIR=~/chroma-target cargo test -p chroma-capture-x11'
```

## Conventions

- Composition and traits only — **no implementation inheritance** (spec §3.2).
- The render pipeline is a deterministic, ordered function of `(Project, t)`
  (spec §3.4). Nothing in the framing path may read wall-clock or unseeded RNG.
- Source media is immutable after capture; effects are evaluated, never baked.
