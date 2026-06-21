# Chroma — Agent Orchestration Guide

> Operating manual for the multi-agent build of **Chroma**, the Linux screen-demo
> capture & compositing studio. Read this in full before doing any work.
> Companion document: `Chroma-Requirements-Architecture.docx` (the spec — the
> source of truth for *what* to build; this file governs *how* the agents build it).

---

## 0. TL;DR for every agent

1. You own **one crate**. Touch only your crate unless a contract change is approved.
2. Depend on `*-api` contract crates, **never** on another implementation crate's internals.
3. Contracts are frozen at M0. Changing one is a **coordination event**, not a code edit (see §6).
4. Composition and traits only. **No implementation inheritance, ever.**
5. A task is "done" only when it builds, is tested in isolation against fakes, and its public surface matches the contract.
6. When blocked on a contract or an unknown, **stop and escalate** — don't invent a workaround that leaks across a boundary.

---

## 1. Roles

The build uses a small set of roles. One agent may hold several on a small team;
on a larger team, split them.

| Role | Model tier | Responsibility |
|------|-----------|----------------|
| **Orchestrator** | Opus | Owns the plan, assigns tasks, guards contracts, runs integration, resolves cross-crate disputes. The only role allowed to approve a contract change. |
| **Architect** | Opus | Authors and freezes the `*-api` contracts at M0; owns the deterministic eval pipeline and camera-blend rules. Advisory thereafter. |
| **Hard-node engineer** | Opus | Owns the high-risk crates: `camera` (smoothing math), `compositor` (wgpu), `capture-*` (clock sync, PipeWire/libei). |
| **Execution engineer** | Sonnet | Owns a breadth crate against a frozen contract: `media-ffmpeg`, `modifiers`, `project`, `app`, `tauri`, `ui`. |
| **Integrator / QA** | Sonnet | Wires crates at milestone boundaries, owns the golden-frame + smoke tests, runs CI. |

> Model-tier guidance mirrors the build-sequence rationale: Opus plans and owns the
> hard nodes; Sonnet executes the breadth. Tiers are per-role, not global.

---

## 2. The non-negotiables

These are invariants. A change to any of them is a coordination event, never a unilateral edit.

- **Contract-first.** Every crate's only public surface is the trait(s) and value types it exposes. Consumers depend on `*-api` crates only.
- **No inheritance.** Shared behavior is a composed helper struct or free function, never a base class or trait default that hides real logic. Trait default methods are allowed only for genuinely-empty no-ops (e.g. an overlay modifier's empty `camera_contribution`).
- **Determinism.** The render pipeline (spec §3.4) is fixed and ordered. Any code that affects framing must be a pure function of `(Project, t)`. No wall-clock, no RNG without a seeded, project-stored seed.
- **One clock.** Capture frames and input events share a single monotonic timebase. Nothing downstream may assume a different timebase.
- **Non-destructive.** Source media is immutable after capture. Effects are evaluated, never baked into the source.

If a task seems to require breaking one of these, that's a signal to escalate — not to proceed.

---

## 3. Ownership map

One owner per crate. Contract crates are owned by the Architect and frozen.

| Crate | Owner role | Depends on (contracts only) | Notes |
|-------|-----------|------------------------------|-------|
| `chroma-core-api` | Architect | — | Frozen at M0. Value types + `Modifier`. |
| `chroma-capture-api` | Architect | core-api | Frozen at M0. |
| `chroma-media-api` | Architect | core-api | Frozen at M0. |
| `chroma-capture-x11` | Hard-node | capture-api | First-target backend. |
| `chroma-capture-wayland` | Hard-node | capture-api | M7. Same traits as x11. |
| `chroma-media-ffmpeg` | Execution | media-api | Decode + encode + GIF palette. |
| `chroma-modifiers` | Execution | core-api | One struct per effect. |
| `chroma-camera` | Hard-node | core-api | Spring smoothing + blend solver. |
| `chroma-compositor` | Hard-node | core-api | wgpu passes. |
| `chroma-render` | Hard-node | core-api, media-api | Wires the §3.4 pipeline. |
| `chroma-project` | Execution | core-api, media-api | Versioned (de)serialization. |
| `chroma-app` | Execution | all `*-api` | Orchestration, commands/undo, schedulers. |
| `chroma-tauri` | Execution | app | IPC bridge. |
| `chroma-ui` | Execution | (tauri IPC contract) | SvelteKit/TypeScript. |

**Rule:** if your task makes you want to edit a crate you don't own, you actually
have a contract gap. Escalate to the Orchestrator.

---

## 4. Milestone gates

Work proceeds milestone by milestone (spec §5). A milestone is **closed** only when
its exit criteria pass in CI. Do not start work that depends on an unclosed milestone's
output unless it's against that milestone's *contract* (which is available from M0).

| Milestone | Exit criteria (all must pass) |
|-----------|-------------------------------|
| **M0 Contracts** | All `*-api` crates compile; fakes for every trait compile; `cargo doc` clean; contracts reviewed + frozen by Orchestrator. |
| **M1 Capture** | Record X11 screen + input (incl. scroll wheel + keystrokes) to a project file; **CAP-05 clock-sync validated to ±1 frame** with a measurement test. |
| **M2 Render** | Source composited over a gradient background, static camera, live preview window holding a frame-time budget (≥30fps target). |
| **M3 Export** | MP4 + GIF export of the M2 composite with the §3.6 settings exposed; golden-frame test passes. |
| **M4 Timeline** | Modifier lanes in UI; Crop/Zoom + Text modifiers add/move/trim/edit end-to-end. |
| **M5 Cursor follow** | Spring-smoothed follow camera + synthetic cursor + click ripple; tunable params. |
| **M6 Polish** | Auto-zoom assist, keyframes, undo/redo, HW encode, presets. |
| **M7 Wayland** | PipeWire + libei backend behind the existing capture contract; no consumer changes. |

**Gate discipline:** the Integrator runs the gate. The Orchestrator declares the
milestone closed. Nobody self-certifies.

---

## 5. Definition of Done (per task)

A task is done when **all** of these hold:

- [ ] Builds clean: `cargo build` / `cargo clippy -- -D warnings` with no new warnings.
- [ ] Formatted: `cargo fmt --check` passes.
- [ ] Public surface matches the owning contract exactly — no extra public items leaking implementation.
- [ ] Unit-tested **in isolation** against fakes (no real GPU/capture device needed for logic tests).
- [ ] No dependency added on another implementation crate's internals.
- [ ] No inheritance introduced; shared logic is composed.
- [ ] If it affects framing: covered by a golden-frame test and deterministic.
- [ ] Doc comment on every public trait/struct/fn explaining the contract, not the implementation.

---

## 6. Contract changes (the one thing that needs coordination)

Contracts are frozen at M0 so agents can work in parallel. They *will* occasionally
need to change. When they do:

1. **Stop.** Do not work around a contract gap inside your crate.
2. **Propose.** Open a `CONTRACT-CHANGE` request to the Orchestrator: the trait/type affected, the reason, and every crate that consumes it.
3. **Assess blast radius.** The Architect lists affected consumers (cheap, because consumers only touch `*-api`).
4. **Approve + version.** The Orchestrator approves; the contract crate's version bumps; the change is announced to all owners of affected crates.
5. **Migrate.** Affected owners update against the new contract before the next gate.

A contract change that touches `core-api` is the most expensive event in the project —
treat the `Modifier` trait and the core value types as near-sacred.

---

## 7. Communication protocol

Keep cross-agent chatter structured and minimal — coupling in conversation becomes
coupling in code.

- **Task handoff:** crate, contract version, exit criteria, deadline/gate. Nothing more.
- **Blocked:** state the blocker as one of `{contract-gap, unknown-env, upstream-incomplete, ambiguous-spec}` and escalate to the Orchestrator. Don't silently improvise across a boundary.
- **Spec ambiguity:** resolve against the `.docx`; if still ambiguous, the Orchestrator decides and records the decision in `DECISIONS.md`.
- **Status:** report against milestone exit criteria, not lines of code.

Every non-obvious cross-crate decision gets one line in `DECISIONS.md` (date, decision,
why). That file is the project's memory.

---

## 8. Known risk register (watch these)

From the spec's risk section — these are where the project actually gets hard. Owners
of these crates should over-communicate.

- **Wayland input capture** (`capture-wayland`, M7) — highest unknown. Deferred behind a stable contract precisely so it can't block v1. Don't let its difficulty leak earlier.
- **Capture/clock sync** (`capture-x11`, M1) — underpins everything; the ±1-frame guarantee must be *measured*, not assumed, before M2 builds on it.
- **GIF quality on gradients** (`media-ffmpeg`, M3) — needs the two-pass palette early to validate the size/quality tradeoff on the colorful backgrounds.
- **Preview performance** (`compositor`/`render`, from M2) — guard with a frame-time budget from the first frame drawn; a slow compositor undermines the whole editor.
- **Env-specific friction** (PipeWire portals, VAAPI/NVENC, HiDPI, X11 vs Wayland permissions) — not an intelligence problem; budget real doc-reading and iteration loops with tool access here.

---

## 9. Repository conventions

- **Layout:** Cargo workspace; one crate per directory under `crates/`; the SvelteKit app under `app/`.
- **Naming:** contract crates end in `-api` and contain only traits + value types + fakes.
- **Tests:** unit tests beside code; golden-frame fixtures under `crates/render/tests/golden/`; a top-level smoke test that records → edits → exports a 5-second clip headlessly.
- **CI:** every PR runs `fmt`, `clippy -D warnings`, unit tests, and (on render-affecting crates) the golden-frame diff. Milestone gates run the full smoke test.
- **Determinism in CI:** golden-frame tests pin encoder settings and compare the *pre-encode* RGBA frames to avoid encoder nondeterminism; encoded-output tests assert structural properties, not byte-equality.

---

## 10. First moves by role (kickoff)

- **Orchestrator:** stand up the workspace + CI skeleton + `DECISIONS.md`; convene M0; do not let any execution agent start until contracts are frozen.
- **Architect:** draft `core-api`, `capture-api`, `media-api` with fakes; get them to compile and `cargo doc` clean; freeze.
- **Hard-node engineers:** while M0 runs, write **fakes and tests** against the draft contracts (the capture clock test, a golden-frame harness with a fake source) so M1/M2 can move the instant contracts freeze.
- **Execution engineers:** build against fakes too — `media-ffmpeg` can encode a synthetic test pattern before any real capture exists; `modifiers` can be unit-tested against a fake `EvalContext`.
- **Integrator:** own the smoke test from day one, even if it just asserts "empty project round-trips."

> The reason fakes come first: they let every agent make real progress against a
> frozen *interface* before any real *implementation* exists. That is the entire
> point of the contract-first design — parallelism with low coupling.
