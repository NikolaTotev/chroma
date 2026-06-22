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
- **GUI — Chroma Studio:** `chroma-studio` is the headless editor engine
  (project + undo history + preview render + export), and `app/` is a
  Tauri + Svelte desktop editor over it (preview, presets, scene styling,
  timeline, undo/redo, export). See [app/README.md](app/README.md).

## Run it (the `chroma` CLI)

Needs `ffmpeg` installed (`sudo apt install ffmpeg`).

```sh
# Render the built-in styled demo (crop-zoom + text) — works anywhere:
cargo run -p chroma-app --bin chroma -- render out.mp4 5 30

# Record the desktop, styled with a cursor-follow camera (native X11 only):
cargo run -p chroma-app --bin chroma -- record recording.mp4 8 30
```

Lower-level demos:

```sh
cargo run -p chroma-render --example compose                  # → out.bmp (one frame)
cargo run -p chroma-media-ffmpeg --example studio -- out.mp4  # → effects clip
```

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
