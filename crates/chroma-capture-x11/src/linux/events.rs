//! XInput2 raw-event source.

use super::be;
use crate::clock::MonotonicClock;
use chroma_capture_api::{
    CaptureError, Clock, EventSource, InputEvent, MouseButton, Result, ScrollDelta, TimedInputEvent,
};
use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{ConnectionExt as _, Window};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

/// XInput2 selects raw events for all master devices on the root window. Device
/// id 1 is `XIAllMasterDevices`.
const XI_ALL_MASTER_DEVICES: u16 = 1;

/// Receives global input events (pointer move, buttons, scroll, keystrokes) via
/// XInput2 raw events, regardless of which window has focus.
///
/// Construct with [`X11EventSource::new`]; [`poll`](EventSource::poll) drains
/// everything seen since the previous call, each stamped on the shared
/// [`MonotonicClock`]. Keystroke events carry the raw keycode and modifier mask
/// for the on-screen overlay (CAP-03); they are never transmitted (spec §3.5).
pub struct X11EventSource {
    conn: RustConnection,
    root: Window,
    clock: MonotonicClock,
}

impl X11EventSource {
    /// Opens a connection, verifies XInput2, and selects raw input events.
    ///
    /// Returns [`CaptureError::Unavailable`] when no X server is reachable, or
    /// [`CaptureError::Backend`] when XInput2 is missing.
    pub fn new() -> Result<Self> {
        let (conn, screen_num) =
            x11rb::connect(None).map_err(|e| CaptureError::Unavailable(e.to_string()))?;
        let root = conn.setup().roots[screen_num].root;

        // Require XInput2 (>= 2.0) before selecting raw events.
        conn.xinput_xi_query_version(2, 0)
            .map_err(be)?
            .reply()
            .map_err(be)?;

        let mask = xinput::XIEventMask::RAW_MOTION
            | xinput::XIEventMask::RAW_BUTTON_PRESS
            | xinput::XIEventMask::RAW_BUTTON_RELEASE
            | xinput::XIEventMask::RAW_KEY_PRESS
            | xinput::XIEventMask::RAW_KEY_RELEASE;
        let event_mask = xinput::EventMask {
            deviceid: XI_ALL_MASTER_DEVICES,
            mask: vec![mask],
        };
        conn.xinput_xi_select_events(root, &[event_mask])
            .map_err(be)?;
        conn.flush().map_err(be)?;

        Ok(X11EventSource {
            conn,
            root,
            clock: MonotonicClock::new(),
        })
    }

    /// Queries the true hardware pointer: `(x, y, modifier_mask)` in root pixel
    /// coordinates (CAP-04). Raw events carry only deltas, so absolute position
    /// and current modifier state are read here.
    fn pointer_state(&self) -> Option<(f32, f32, u32)> {
        let p = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
        Some((
            p.root_x as f32,
            p.root_y as f32,
            u32::from(u16::from(p.mask)),
        ))
    }

    /// Translates one X event into a Chroma [`InputEvent`], or `None` if it is
    /// not one we model.
    fn translate(&self, event: Event) -> Option<InputEvent> {
        match event {
            Event::XinputRawMotion(_) => {
                let (x, y, _) = self.pointer_state()?;
                Some(InputEvent::PointerMove { x, y })
            }
            Event::XinputRawButtonPress(ev) => {
                if let Some(delta) = scroll_delta(ev.detail) {
                    Some(InputEvent::Scroll { delta })
                } else {
                    let (x, y, _) = self.pointer_state().unwrap_or((0.0, 0.0, 0));
                    Some(InputEvent::ButtonDown {
                        button: map_button(ev.detail),
                        x,
                        y,
                    })
                }
            }
            Event::XinputRawButtonRelease(ev) => {
                // Scroll "buttons" have no meaningful release.
                if scroll_delta(ev.detail).is_some() {
                    return None;
                }
                let (x, y, _) = self.pointer_state().unwrap_or((0.0, 0.0, 0));
                Some(InputEvent::ButtonUp {
                    button: map_button(ev.detail),
                    x,
                    y,
                })
            }
            Event::XinputRawKeyPress(ev) => Some(InputEvent::KeyDown {
                keycode: ev.detail,
                modifiers: self.pointer_state().map_or(0, |(_, _, m)| m),
            }),
            Event::XinputRawKeyRelease(ev) => Some(InputEvent::KeyUp {
                keycode: ev.detail,
                modifiers: self.pointer_state().map_or(0, |(_, _, m)| m),
            }),
            _ => None,
        }
    }
}

impl EventSource for X11EventSource {
    fn poll(&mut self) -> Vec<TimedInputEvent> {
        let mut out = Vec::new();
        while let Ok(Some(event)) = self.conn.poll_for_event() {
            if let Some(input) = self.translate(event) {
                out.push(TimedInputEvent {
                    timestamp: self.clock.now(),
                    event: input,
                });
            }
        }
        out
    }
}

/// Maps an X button number to a [`MouseButton`] (1=left, 2=middle, 3=right).
fn map_button(detail: u32) -> MouseButton {
    match detail {
        1 => MouseButton::Left,
        2 => MouseButton::Middle,
        3 => MouseButton::Right,
        other => MouseButton::Other(other as u16),
    }
}

/// Maps X scroll "buttons" (4–7) to a [`ScrollDelta`], or `None` for real
/// buttons. 4=up, 5=down, 6=left, 7=right.
fn scroll_delta(detail: u32) -> Option<ScrollDelta> {
    match detail {
        4 => Some(ScrollDelta { dy: 1.0, dx: 0.0 }),
        5 => Some(ScrollDelta { dy: -1.0, dx: 0.0 }),
        6 => Some(ScrollDelta { dy: 0.0, dx: -1.0 }),
        7 => Some(ScrollDelta { dy: 0.0, dx: 1.0 }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_map_correctly() {
        assert_eq!(map_button(1), MouseButton::Left);
        assert_eq!(map_button(3), MouseButton::Right);
        assert_eq!(map_button(9), MouseButton::Other(9));
    }

    #[test]
    fn scroll_buttons_are_deltas_not_clicks() {
        assert_eq!(scroll_delta(4), Some(ScrollDelta { dy: 1.0, dx: 0.0 }));
        assert_eq!(scroll_delta(5), Some(ScrollDelta { dy: -1.0, dx: 0.0 }));
        assert!(scroll_delta(1).is_none());
    }
}
