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
- **M2 — Render core (in progress):** `chroma-compositor` (CPU) composites a
  background + camera-transformed scene inset; `chroma-render` wires the §3.4
  pipeline into one deterministic composited frame. (Live preview window and the
  wgpu compositor are still to come.)

Remaining implementation crates land milestone by milestone (ORCHESTRATION.md §4).

To see a composited frame, render the demo and open the BMP:

```sh
cargo run -p chroma-render --example compose      # writes out.bmp
```

## Workspace layout

```
crates/
  chroma-core-api/     # value types + Modifier/Compositor traits (no logic)
  chroma-capture-api/  # ScreenCapturer, EventSource, Clock + Frame/InputEvent
  chroma-media-api/    # Decoder, Encoder, FrameSource + codec/param types
  chroma-capture-x11/  # X11 capture backend (Linux); stub elsewhere
  chroma-compositor/   # CPU reference compositor (implements Compositor)
  chroma-render/       # deterministic §3.4 render pipeline → composited frame
```

Contract crates hold **only** traits, value types, and fakes. Implementation
crates depend on the `-api` crates, never on each other's internals.

Deferred (added at their milestones — see DECISIONS.md): `chroma-capture-wayland`
(M7) plus the logic crates `chroma-media-ffmpeg`, `chroma-modifiers`,
`chroma-camera`, `chroma-project`, `chroma-app`, `chroma-tauri`, and the
SvelteKit front end under `app/`.

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
