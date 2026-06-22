// TypeScript mirrors of the `chroma-core-api` serde shapes. Rust enums are
// externally tagged by default (e.g. `Background::Solid([..])` serializes as
// `{ "Solid": [r,g,b,a] }`), and `TimeStamp(u64)` is a newtype that serializes
// as a bare number. These types match that wire format exactly.

export type Rgba = [number, number, number, number];

export interface Point {
  x: number;
  y: number;
}
export interface Size {
  width: number;
  height: number;
}
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Nanoseconds on the project clock (Rust `TimeStamp(u64)`). */
export type TimeStamp = number;
export interface TimeRange {
  start: TimeStamp;
  end: TimeStamp;
}

export interface GradientStop {
  offset: number;
  rgba: Rgba;
}
export type Background =
  | { Solid: Rgba }
  | { Gradient: { angle_deg: number; stops: GradientStop[] } }
  | { Image: { path: string } };

export interface Shadow {
  dx: number;
  dy: number;
  blur: number;
  rgba: Rgba;
}
export interface Border {
  width: number;
  rgba: Rgba;
}
export interface SceneStyle {
  padding: number;
  corner_radius: number;
  shadow: Shadow | null;
  border: Border | null;
}

export type ModifierKind = "Camera" | "Overlay";

export type ModifierParams =
  | { CropZoom: { target: Rect } }
  | { Text: { content: string; rect: Rect; rgba: Rgba } }
  | { CursorFollow: { zoom: number; tightness: number } }
  | { Highlight: { radius: number } }
  | { CursorMarker: { size: number } }
  | { ClickRipple: { center: Point; max_radius: number } }
  | {
      KeyframeCamera: {
        center_x: { keys: unknown[] };
        center_y: { keys: unknown[] };
        scale: { keys: unknown[] };
        weight: number;
      };
    };

export interface ModifierSpec {
  kind: ModifierKind;
  range: TimeRange;
  params: ModifierParams;
}

export interface SourceMedia {
  video_path: string;
  event_log_path: string;
  fps: number;
  size: Size;
}

export interface Project {
  version: number;
  source: SourceMedia;
  canvas: Size;
  background: Background;
  scene: SceneStyle;
  modifiers: ModifierSpec[];
}

/** A snapshot of editor state returned by every mutating command. */
export interface StudioState {
  project: Project;
  can_undo: boolean;
  can_redo: boolean;
  duration_ns: number;
  presets: string[];
  is_recording: boolean;
  record_elapsed_ns: number;
}

/** A short human label for a modifier's params, for the timeline. */
export function modifierLabel(spec: ModifierSpec): string {
  const p = spec.params;
  if ("CropZoom" in p) return "Crop / Zoom";
  if ("Text" in p) return `Text “${p.Text.content}”`;
  if ("CursorFollow" in p) return `Cursor Follow ×${p.CursorFollow.zoom}`;
  if ("Highlight" in p) return "Highlight";
  if ("CursorMarker" in p) return "Cursor Marker";
  if ("ClickRipple" in p) return "Click Ripple";
  if ("KeyframeCamera" in p) return "Keyframe Camera";
  return "Modifier";
}
