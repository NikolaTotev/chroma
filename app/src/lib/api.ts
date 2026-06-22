// The editor's data access layer.
//
// In the packaged desktop app these call Rust commands over Tauri's `invoke`.
// In a plain browser (dev, `vite preview`, or this repo's CI build) there is no
// backend, so a self-contained mock studio keeps the whole UI interactive —
// the component code is identical in both worlds.

import type {
  Background,
  ModifierSpec,
  Project,
  SceneStyle,
  StudioState,
} from "./types";

const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

// ---------------------------------------------------------------------------
// Public API — each mutating call returns the fresh StudioState.
// ---------------------------------------------------------------------------

export const api = {
  inTauri,

  state(): Promise<StudioState> {
    return inTauri ? invoke("studio_state") : mock.state();
  },
  renderPreview(timeNs: number): Promise<string> {
    return inTauri ? invoke("render_preview", { timeNs }) : mock.renderPreview(timeNs);
  },
  newProject(): Promise<StudioState> {
    return inTauri ? invoke("new_project") : mock.newProject();
  },
  applyPreset(name: string): Promise<StudioState> {
    return inTauri ? invoke("apply_preset", { name }) : mock.applyPreset(name);
  },
  setSolidBackground(rgba: [number, number, number, number]): Promise<StudioState> {
    const bg: Background = { Solid: rgba };
    return inTauri ? invoke("set_background", { bg }) : mock.setBackground(bg);
  },
  setBackground(bg: Background): Promise<StudioState> {
    return inTauri ? invoke("set_background", { bg }) : mock.setBackground(bg);
  },
  setScene(scene: SceneStyle): Promise<StudioState> {
    return inTauri ? invoke("set_scene", { scene }) : mock.setScene(scene);
  },
  addModifier(spec: ModifierSpec): Promise<StudioState> {
    return inTauri ? invoke("add_modifier", { spec }) : mock.addModifier(spec);
  },
  removeModifier(index: number): Promise<StudioState> {
    return inTauri ? invoke("remove_modifier", { index }) : mock.removeModifier(index);
  },
  undo(): Promise<StudioState> {
    return inTauri ? invoke("undo") : mock.undo();
  },
  redo(): Promise<StudioState> {
    return inTauri ? invoke("redo") : mock.redo();
  },
  startRecord(fps: number): Promise<StudioState> {
    return inTauri ? invoke("start_record", { fps }) : mock.startRecord();
  },
  stopRecord(): Promise<StudioState> {
    return inTauri ? invoke("stop_record") : mock.stopRecord();
  },
  save(path: string): Promise<StudioState> {
    return inTauri ? invoke("save_project", { path }) : mock.state();
  },
  open(path: string): Promise<StudioState> {
    return inTauri ? invoke("open_project", { path }) : mock.state();
  },
  exportVideo(path: string, secs: number, fps: number): Promise<string> {
    return inTauri
      ? invoke("export_video", { path, secs, fps })
      : Promise.resolve(`(mock) would export ${path}`);
  },
};

// ---------------------------------------------------------------------------
// Browser mock: a minimal in-memory studio mirroring the Rust engine's shape.
// ---------------------------------------------------------------------------

const PRESETS: Record<string, { bg: Background; scene: SceneStyle }> = {
  Clean: {
    bg: { Solid: [0.96, 0.96, 0.97, 1] },
    scene: { padding: 0.05, corner_radius: 0.03, shadow: { dx: 0, dy: 0.012, blur: 0.025, rgba: [0, 0, 0, 0.25] }, border: null },
  },
  Vibrant: {
    bg: { Gradient: { angle_deg: 35, stops: [ { offset: 0, rgba: [0.08, 0.1, 0.28, 1] }, { offset: 1, rgba: [0.5, 0.16, 0.42, 1] } ] } },
    scene: { padding: 0.07, corner_radius: 0.05, shadow: { dx: 0, dy: 0.02, blur: 0.04, rgba: [0, 0, 0, 0.5] }, border: null },
  },
  Spotlight: {
    bg: { Gradient: { angle_deg: 90, stops: [ { offset: 0, rgba: [0.02, 0.02, 0.03, 1] }, { offset: 1, rgba: [0.12, 0.12, 0.16, 1] } ] } },
    scene: { padding: 0.09, corner_radius: 0.06, shadow: { dx: 0, dy: 0.03, blur: 0.06, rgba: [0, 0, 0, 0.6] }, border: null },
  },
};

function defaultProject(): Project {
  return {
    version: 1,
    source: { video_path: "", event_log_path: "", fps: 30, size: { width: 1280, height: 720 } },
    canvas: { width: 1280, height: 720 },
    background: PRESETS.Vibrant.bg,
    scene: PRESETS.Vibrant.scene,
    modifiers: [
      { kind: "Camera", range: { start: 300000000, end: 5700000000 }, params: { CursorFollow: { zoom: 1.5, tightness: 1 } } },
      { kind: "Overlay", range: { start: 0, end: 6000000000 }, params: { CursorMarker: { size: 0.05 } } },
      { kind: "Overlay", range: { start: 500000000, end: 5500000000 }, params: { Text: { content: "Chroma", rect: { x: 0.3, y: 0.82, width: 0.4, height: 0.1 }, rgba: [1, 1, 1, 1] } } },
    ],
  };
}

const mockStore = {
  project: defaultProject(),
  undo: [] as Project[],
  redo: [] as Project[],
  recording: false,
  recordStart: 0,
};

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v));
}

function push() {
  mockStore.undo.push(clone(mockStore.project));
  mockStore.redo = [];
}

function snapshot(): StudioState {
  const ends = mockStore.project.modifiers
    .map((m) => m.range.end)
    .filter((e) => e < Number.MAX_SAFE_INTEGER / 2);
  const duration = Math.max(2e9, ends.length ? Math.max(...ends) : 6e9);
  return {
    project: clone(mockStore.project),
    can_undo: mockStore.undo.length > 0,
    can_redo: mockStore.redo.length > 0,
    duration_ns: duration,
    presets: Object.keys(PRESETS),
    is_recording: mockStore.recording,
    record_elapsed_ns: mockStore.recording ? (Date.now() - mockStore.recordStart) * 1e6 : 0,
  };
}

const mock = {
  state: async () => snapshot(),
  newProject: async () => {
    push();
    mockStore.project = defaultProject();
    return snapshot();
  },
  applyPreset: async (name: string) => {
    const p = PRESETS[name];
    if (p) {
      push();
      mockStore.project.background = clone(p.bg);
      mockStore.project.scene = clone(p.scene);
    }
    return snapshot();
  },
  setBackground: async (bg: Background) => {
    push();
    mockStore.project.background = clone(bg);
    return snapshot();
  },
  setScene: async (scene: SceneStyle) => {
    push();
    mockStore.project.scene = clone(scene);
    return snapshot();
  },
  addModifier: async (spec: ModifierSpec) => {
    push();
    mockStore.project.modifiers.push(clone(spec));
    return snapshot();
  },
  removeModifier: async (index: number) => {
    push();
    mockStore.project.modifiers.splice(index, 1);
    return snapshot();
  },
  undo: async () => {
    const prev = mockStore.undo.pop();
    if (prev) {
      mockStore.redo.push(clone(mockStore.project));
      mockStore.project = prev;
    }
    return snapshot();
  },
  redo: async () => {
    const next = mockStore.redo.pop();
    if (next) {
      mockStore.undo.push(clone(mockStore.project));
      mockStore.project = next;
    }
    return snapshot();
  },
  // The browser mock can't capture the screen; it just toggles the flag so the
  // recording UI is exercisable. (The desktop app does the real capture.)
  startRecord: async () => {
    mockStore.recording = true;
    mockStore.recordStart = Date.now();
    return snapshot();
  },
  stopRecord: async () => {
    mockStore.recording = false;
    return snapshot();
  },
  // A representative preview drawn on a canvas, so the browser build shows a
  // believable frame without the Rust compositor.
  renderPreview: async (timeNs: number): Promise<string> => {
    const c = document.createElement("canvas");
    c.width = 640;
    c.height = 360;
    const g = c.getContext("2d")!;
    const bg = mockStore.project.background;
    if ("Solid" in bg) {
      g.fillStyle = rgbaCss(bg.Solid);
    } else if ("Gradient" in bg) {
      const grad = g.createLinearGradient(0, 0, c.width, c.height);
      for (const s of bg.Gradient.stops) grad.addColorStop(s.offset, rgbaCss(s.rgba));
      g.fillStyle = grad;
    } else {
      g.fillStyle = "#222";
    }
    g.fillRect(0, 0, c.width, c.height);
    const pad = mockStore.project.scene.padding * c.height;
    const r = mockStore.project.scene.corner_radius * Math.min(c.width, c.height);
    roundRect(g, pad, pad, c.width - 2 * pad, c.height - 2 * pad, r);
    g.fillStyle = "#0e1014";
    g.fill();
    // A moving marker so scrubbing the timeline visibly changes the frame.
    const t = timeNs / 1e9;
    const mx = c.width * (0.5 + 0.28 * Math.sin(t * 1.3));
    const my = c.height * (0.5 + 0.22 * Math.sin(t * 0.9 + 1));
    g.fillStyle = "#fff";
    g.beginPath();
    g.arc(mx, my, 6, 0, Math.PI * 2);
    g.fill();
    return c.toDataURL("image/png");
  },
};

function rgbaCss([r, g, b, a]: [number, number, number, number]): string {
  return `rgba(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)}, ${a})`;
}

function roundRect(
  g: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
) {
  g.beginPath();
  g.moveTo(x + r, y);
  g.arcTo(x + w, y, x + w, y + h, r);
  g.arcTo(x + w, y + h, x, y + h, r);
  g.arcTo(x, y + h, x, y, r);
  g.arcTo(x, y, x + w, y, r);
  g.closePath();
}
