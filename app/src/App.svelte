<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "./lib/api";
  import { modifierLabel, type ModifierSpec, type SceneStyle, type StudioState } from "./lib/types";

  // Note: not named `state` — that would collide with the `$state` rune in
  // svelte2tsx (it would parse `$state` as a store subscription).
  let view: StudioState | null = $state(null);
  let previewUrl = $state("");
  let time = $state(0); // ns
  let busy = $state(false);
  let status = $state("");

  const fmtSecs = (ns: number) => (ns / 1e9).toFixed(2) + "s";

  async function apply(p: Promise<StudioState>) {
    busy = true;
    try {
      view = await p;
      await rerender();
    } catch (e) {
      status = String(e);
    } finally {
      busy = false;
    }
  }

  async function rerender() {
    previewUrl = await api.renderPreview(Math.round(time));
  }

  onMount(async () => {
    view = await api.state();
    await rerender();
  });

  // --- background ---------------------------------------------------------
  function hexToRgba(hex: string): [number, number, number, number] {
    const n = parseInt(hex.slice(1), 16);
    return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255, 1];
  }
  function rgbaToHex(c: [number, number, number, number]): string {
    const h = (v: number) => Math.round(v * 255).toString(16).padStart(2, "0");
    return `#${h(c[0])}${h(c[1])}${h(c[2])}`;
  }
  let solidHex = $derived.by(() => {
    const bg = view?.project.background;
    return bg && "Solid" in bg ? rgbaToHex(bg.Solid) : "#14161c";
  });

  // --- scene --------------------------------------------------------------
  function commitScene(patch: Partial<SceneStyle>) {
    if (!view) return;
    const scene = { ...view.project.scene, ...patch };
    apply(api.setScene(scene));
  }

  // --- timeline -----------------------------------------------------------
  function addModifier(kind: string) {
    const dur = view?.duration_ns ?? 6e9;
    const full = { start: 0, end: dur };
    const specs: Record<string, ModifierSpec> = {
      Text: { kind: "Overlay", range: { start: 0, end: dur }, params: { Text: { content: "New text", rect: { x: 0.3, y: 0.4, width: 0.4, height: 0.12 }, rgba: [1, 1, 1, 1] } } },
      "Cursor Follow": { kind: "Camera", range: full, params: { CursorFollow: { zoom: 1.6, tightness: 1 } } },
      Highlight: { kind: "Overlay", range: full, params: { Highlight: { radius: 0.08 } } },
      "Cursor Marker": { kind: "Overlay", range: full, params: { CursorMarker: { size: 0.05 } } },
      "Click Ripple": { kind: "Overlay", range: { start: 0, end: 600000000 }, params: { ClickRipple: { center: { x: 0.5, y: 0.5 }, max_radius: 0.12 } } },
    };
    const spec = specs[kind];
    if (spec) apply(api.addModifier(spec));
  }

  let addChoice = $state("Text");

  // --- top bar actions ----------------------------------------------------
  async function doExport() {
    const path = prompt("Export to file (.mp4 or .gif):", "chroma-export.mp4");
    if (!path) return;
    busy = true;
    status = "Exporting…";
    try {
      status = await api.exportVideo(path, Math.round((view?.duration_ns ?? 6e9) / 1e9), 30);
    } catch (e) {
      status = "Export failed: " + String(e);
    } finally {
      busy = false;
    }
  }
  function doSave() {
    const path = prompt("Save project to:", "project.chroma.json");
    if (path) apply(api.save(path));
  }
  function doOpen() {
    const path = prompt("Open project from:", "project.chroma.json");
    if (path) apply(api.open(path));
  }

  // --- recording ----------------------------------------------------------
  let recElapsed = $state(0); // seconds
  let recTimer: ReturnType<typeof setInterval> | null = null;
  const fmtClock = (s: number) =>
    `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;

  async function toggleRecord() {
    if (view?.is_recording) {
      if (recTimer) { clearInterval(recTimer); recTimer = null; }
      status = "Loading recording…";
      await apply(api.stopRecord()); // refreshes state + preview (now real footage)
      status = "";
      time = 0;
    } else {
      status = "";
      try {
        view = await api.startRecord(30);
        recElapsed = 0;
        recTimer = setInterval(() => (recElapsed += 1), 1000);
      } catch (e) {
        status = "Record failed: " + String(e);
      }
    }
  }
</script>

<div class="app">
  <header>
    <div class="brand">
      <span class="dot"></span> Chroma <span class="muted">Studio</span>
    </div>
    {#if view?.is_recording}
      <button class="rec recording" onclick={toggleRecord}>■ Stop {fmtClock(recElapsed)}</button>
    {:else}
      <button class="rec" onclick={toggleRecord} disabled={busy}>● Record</button>
    {/if}
    <div class="spacer"></div>
    <button onclick={() => apply(api.newProject())} disabled={busy}>New</button>
    <button onclick={doOpen} disabled={busy}>Open</button>
    <button onclick={doSave} disabled={busy}>Save</button>
    <div class="sep"></div>
    <button onclick={() => apply(api.undo())} disabled={busy || !view?.can_undo}>↶ Undo</button>
    <button onclick={() => apply(api.redo())} disabled={busy || !view?.can_redo}>↷ Redo</button>
    <div class="sep"></div>
    <button class="primary" onclick={doExport} disabled={busy}>Export</button>
  </header>

  <div class="body">
    <aside class="left">
      <section>
        <h2>Presets</h2>
        <div class="presets">
          {#each view?.presets ?? [] as name}
            <button onclick={() => apply(api.applyPreset(name))} disabled={busy}>{name}</button>
          {/each}
        </div>
      </section>

      <section>
        <h2>Background</h2>
        <div class="row">
          <label for="bgcolor">Solid color</label>
          <input
            id="bgcolor"
            type="color"
            value={solidHex}
            onchange={(e) => apply(api.setSolidBackground(hexToRgba(e.currentTarget.value)))}
          />
        </div>
        <p class="hint">Pick a preset above for gradient looks.</p>
      </section>

      {#if view}
        <section>
          <h2>Scene style</h2>
          <div class="slider">Padding <span>{view.project.scene.padding.toFixed(3)}</span></div>
          <input type="range" min="0" max="0.2" step="0.005" aria-label="Padding"
            value={view.project.scene.padding}
            onchange={(e) => commitScene({ padding: +e.currentTarget.value })} />

          <div class="slider">Corner radius <span>{view.project.scene.corner_radius.toFixed(3)}</span></div>
          <input type="range" min="0" max="0.2" step="0.005" aria-label="Corner radius"
            value={view.project.scene.corner_radius}
            onchange={(e) => commitScene({ corner_radius: +e.currentTarget.value })} />

          <div class="slider">Shadow blur <span>{(view.project.scene.shadow?.blur ?? 0).toFixed(3)}</span></div>
          <input type="range" min="0" max="0.12" step="0.005" aria-label="Shadow blur"
            value={view.project.scene.shadow?.blur ?? 0}
            onchange={(e) => commitScene({ shadow: { dx: 0, dy: view!.project.scene.shadow?.dy ?? 0.02, blur: +e.currentTarget.value, rgba: view!.project.scene.shadow?.rgba ?? [0, 0, 0, 0.5] } })} />
        </section>
      {/if}
    </aside>

    <main class="stage">
      <div class="preview">
        {#if previewUrl}
          <img src={previewUrl} alt="composited preview" />
        {:else}
          <div class="placeholder">Rendering…</div>
        {/if}
      </div>
      <div class="scrub">
        <input type="range" min="0" max={view?.duration_ns ?? 6e9} step="33333333" aria-label="Timeline scrubber"
          value={time}
          oninput={(e) => { time = +e.currentTarget.value; rerender(); }} />
        <span class="time">{fmtSecs(time)} / {fmtSecs(view?.duration_ns ?? 0)}</span>
      </div>
      <div class="statusbar">
        <span class="badge">{api.inTauri ? "desktop" : "preview (mock)"}</span>
        <span class="muted">{status}</span>
      </div>
    </main>
  </div>

  <footer class="timeline">
    <div class="tl-head">
      <h2>Timeline</h2>
      <div class="add">
        <select bind:value={addChoice} aria-label="Modifier type to add">
          {#each ["Text", "Cursor Follow", "Highlight", "Cursor Marker", "Click Ripple"] as t}
            <option>{t}</option>
          {/each}
        </select>
        <button onclick={() => addModifier(addChoice)} disabled={busy}>+ Add</button>
      </div>
    </div>
    <div class="lanes">
      {#each view?.project.modifiers ?? [] as spec, i}
        <div class="lane">
          <span class="kind {spec.kind.toLowerCase()}">{spec.kind}</span>
          <span class="label">{modifierLabel(spec)}</span>
          <span class="range">{fmtSecs(spec.range.start)} → {spec.range.end > 9e18 ? "∞" : fmtSecs(spec.range.end)}</span>
          <button class="del" onclick={() => apply(api.removeModifier(i))} disabled={busy} aria-label="remove">✕</button>
        </div>
      {:else}
        <p class="hint">No modifiers yet — add one above.</p>
      {/each}
    </div>
  </footer>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .brand {
    font-weight: 700;
    letter-spacing: 0.02em;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .brand .muted {
    color: var(--muted);
    font-weight: 500;
  }
  .dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: linear-gradient(135deg, var(--accent), var(--accent-2));
    box-shadow: 0 0 12px var(--accent);
  }
  .spacer {
    flex: 1;
  }
  .sep {
    width: 1px;
    height: 22px;
    background: var(--line);
    margin: 0 4px;
  }
  .muted {
    color: var(--muted);
  }

  .rec {
    border-color: #4a2b34;
    color: #ff8aa0;
  }
  .rec.recording {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
    font-weight: 600;
    animation: pulse 1.4s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      box-shadow: 0 0 0 4px rgba(224, 86, 111, 0.25);
    }
  }

  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .left {
    width: 280px;
    flex-shrink: 0;
    background: var(--panel);
    border-right: 1px solid var(--line);
    overflow-y: auto;
    padding: 16px;
  }
  .left section {
    margin-bottom: 22px;
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .hint {
    color: var(--muted);
    font-size: 12px;
    margin: 8px 0 0;
  }
  .slider {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    color: var(--muted);
    margin: 12px 0 4px;
  }
  .slider span {
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .stage {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: 16px;
    gap: 12px;
  }
  .preview {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: repeating-conic-gradient(#15171d 0% 25%, #111319 0% 50%) 0 / 24px 24px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
    min-height: 0;
  }
  .preview img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    box-shadow: 0 16px 50px rgba(0, 0, 0, 0.5);
  }
  .placeholder {
    color: var(--muted);
  }
  .scrub {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .time {
    font-variant-numeric: tabular-nums;
    color: var(--muted);
    white-space: nowrap;
    font-size: 12px;
  }
  .statusbar {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 18px;
  }
  .badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    color: var(--muted);
  }

  .timeline {
    height: 200px;
    background: var(--panel);
    border-top: 1px solid var(--line);
    padding: 12px 16px;
    overflow-y: auto;
  }
  .tl-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .add {
    display: flex;
    gap: 8px;
  }
  .add select {
    background: var(--panel-2);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 6px 8px;
  }
  .lanes {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .lane {
    display: flex;
    align-items: center;
    gap: 12px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 8px 12px;
  }
  .kind {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 2px 8px;
    border-radius: 999px;
  }
  .kind.camera {
    background: rgba(124, 92, 255, 0.18);
    color: #b6a4ff;
  }
  .kind.overlay {
    background: rgba(196, 75, 214, 0.18);
    color: #e9a6f2;
  }
  .lane .label {
    flex: 1;
  }
  .lane .range {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    font-size: 12px;
  }
  .del {
    padding: 2px 8px;
    color: var(--danger);
  }
</style>
