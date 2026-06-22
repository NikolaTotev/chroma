//! The Wayland capture design: XDG desktop portals + PipeWire + libei.
//!
//! Wayland deliberately denies clients the ability to read other windows' or the
//! desktop's pixels directly (the security model the spec calls out at M7). The
//! sanctioned path is the **XDG desktop portals**, brokered over D-Bus by the
//! compositor, which hand back a **PipeWire** stream the client decodes. This
//! module documents the exact flow the `portal` feature implements; it is the
//! Wayland analogue of the X11 backend's `GetImage` + XInput2.
//!
//! # Screen frames — `org.freedesktop.portal.ScreenCast`
//!
//! 1. `CreateSession` → a portal session handle.
//! 2. `SelectSources` — request `types = MONITOR` (full screen) or `WINDOW`,
//!    `cursor_mode = Embedded`/`Metadata`. Mapping from
//!    [`CaptureTarget`](chroma_capture_api::CaptureTarget): `FullScreen` →
//!    `MONITOR`, `Window` → `WINDOW`, `Region` → `MONITOR` then crop in the
//!    compositor (the portal has no sub-monitor region selection).
//! 3. `Start` — the compositor shows the picker; on approval the response
//!    carries one or more PipeWire **node ids** plus the stream size/position.
//! 4. `OpenPipeWireRemote` → a file descriptor for the PipeWire daemon.
//! 5. Connect a PipeWire stream to the node id and pull buffers. Negotiate
//!    `SPA_VIDEO_FORMAT_BGRA`/`RGBA`; each buffer becomes a
//!    [`Frame`](chroma_capture_api::Frame) stamped via [`MonotonicClock`] at
//!    dequeue time, matching the X11 backend's frame contract (stride included).
//!
//! # Input events — `org.freedesktop.portal.RemoteDesktop` / libei
//!
//! Global input on Wayland is likewise mediated. Two options behind the same
//! [`EventSource`](chroma_capture_api::EventSource) contract:
//!
//! - **RemoteDesktop portal**: bind alongside the ScreenCast session
//!   (`ConnectToEIS`) and read pointer/keyboard notifies. Pointer motion is
//!   relative within the stream; absolute position is reconstructed from the
//!   stream geometry, normalized like the X11 path.
//! - **libei** (`ei_*`): the emerging standard for portal-brokered input; the
//!   portal returns an EIS socket and libei surfaces device events, which map
//!   onto [`InputEvent`](chroma_capture_api::InputEvent) (PointerMove,
//!   ButtonDown/Up, Scroll, KeyDown/Up).
//!
//! # One clock
//!
//! PipeWire exposes per-buffer presentation timestamps, but to honor CAP-05 the
//! backend stamps frames and input on the same process-global [`MonotonicClock`]
//! as every other backend — the editor never reasons about two timebases.
//!
//! # Status
//!
//! The flow above is implemented under the `portal` Cargo feature (it pulls
//! `ashpd` + `pipewire`, which need a live Wayland session and
//! `libpipewire-0.3-dev`). With the feature off — the default, and the only
//! thing CI/WSL can build — the backend reports
//! [`Unavailable`](chroma_capture_api::CaptureError::Unavailable). See
//! `DECISIONS.md`.

/// The portal source type requested for a capture, derived from a
/// [`CaptureTarget`](chroma_capture_api::CaptureTarget).
///
/// Kept as a plain enum (no portal dependency) so target-mapping logic is
/// unit-testable without a Wayland session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    /// A whole monitor (`FullScreen`, or `Region` cropped after capture).
    Monitor,
    /// A single application window.
    Window,
}

/// Maps a capture target to the portal `SelectSources` type.
pub fn source_type_for(target: &chroma_capture_api::CaptureTarget) -> SourceType {
    use chroma_capture_api::CaptureTarget;
    match target {
        CaptureTarget::Window { .. } => SourceType::Window,
        // FullScreen and Region both capture a monitor; Region is cropped by the
        // compositor since the portal cannot select a sub-monitor rectangle.
        CaptureTarget::FullScreen { .. } | CaptureTarget::Region { .. } => SourceType::Monitor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chroma_capture_api::CaptureTarget;

    #[test]
    fn maps_targets_to_portal_sources() {
        assert_eq!(
            source_type_for(&CaptureTarget::FullScreen { monitor: 0 }),
            SourceType::Monitor
        );
        assert_eq!(
            source_type_for(&CaptureTarget::Window { id: 7 }),
            SourceType::Window
        );
        assert_eq!(
            source_type_for(&CaptureTarget::Region {
                x: 0,
                y: 0,
                width: 100,
                height: 100
            }),
            SourceType::Monitor
        );
    }
}
