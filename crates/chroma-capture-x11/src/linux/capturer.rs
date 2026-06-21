//! GetImage-based screen frame source.

use super::be;
use crate::clock::MonotonicClock;
use chroma_capture_api::{
    CaptureError, CaptureTarget, Clock, Frame, PixelFormat, Result, ScreenCapturer,
};
use chroma_core_api::Size;
use std::thread::sleep;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, Drawable, ImageFormat};
use x11rb::rust_connection::RustConnection;

/// The fixed grab rectangle and pacing resolved at [`ScreenCapturer::start`].
struct GrabSpec {
    drawable: Drawable,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    /// Nanoseconds between frames (`0` = grab as fast as called).
    interval_nanos: u64,
}

/// Captures screen frames from an X11 server via `GetImage` (ZPixmap).
///
/// Connect with [`X11ScreenCapturer::new`], then [`start`](ScreenCapturer::start)
/// a target and pull frames with [`next_frame`](ScreenCapturer::next_frame).
/// Each frame is stamped on the shared [`MonotonicClock`].
pub struct X11ScreenCapturer {
    conn: RustConnection,
    screen_num: usize,
    clock: MonotonicClock,
    grab: Option<GrabSpec>,
    /// Monotonic nanos at which the next frame is due, for fps pacing.
    next_deadline: Option<u64>,
}

impl X11ScreenCapturer {
    /// Opens a connection to the X server named by `$DISPLAY`.
    ///
    /// Returns [`CaptureError::Unavailable`] when no X server is reachable (e.g.
    /// a pure-Wayland session), so callers can fall back gracefully (CAP-07).
    pub fn new() -> Result<Self> {
        let (conn, screen_num) =
            x11rb::connect(None).map_err(|e| CaptureError::Unavailable(e.to_string()))?;
        Ok(X11ScreenCapturer {
            conn,
            screen_num,
            clock: MonotonicClock::new(),
            grab: None,
            next_deadline: None,
        })
    }
}

impl ScreenCapturer for X11ScreenCapturer {
    fn start(&mut self, target: CaptureTarget, fps: u32) -> Result<()> {
        let screen = &self.conn.setup().roots[self.screen_num];
        let root = screen.root;

        let (drawable, x, y, width, height) = match target {
            // Monitor selection beyond the primary root extent is deferred to a
            // RandR-aware revision; M1 captures the root surface.
            CaptureTarget::FullScreen { .. } => {
                (root, 0, 0, screen.width_in_pixels, screen.height_in_pixels)
            }
            CaptureTarget::Region {
                x,
                y,
                width,
                height,
            } => (root, x as i16, y as i16, width as u16, height as u16),
            CaptureTarget::Window { id } => {
                let drawable = id as Drawable;
                let geom = self
                    .conn
                    .get_geometry(drawable)
                    .map_err(be)?
                    .reply()
                    .map_err(be)?;
                (drawable, 0, 0, geom.width, geom.height)
            }
        };

        if width == 0 || height == 0 {
            return Err(CaptureError::Backend(
                "capture target has zero area".to_owned(),
            ));
        }

        let interval_nanos = if fps == 0 {
            0
        } else {
            1_000_000_000 / fps as u64
        };
        self.grab = Some(GrabSpec {
            drawable,
            x,
            y,
            width,
            height,
            interval_nanos,
        });
        self.next_deadline = None;
        Ok(())
    }

    fn next_frame(&mut self) -> Result<Frame> {
        let grab = self
            .grab
            .as_ref()
            .ok_or_else(|| CaptureError::Backend("next_frame called before start".to_owned()))?;

        // Pace to the requested fps against the shared clock.
        let now = self.clock.now().as_nanos();
        let deadline = self.next_deadline.unwrap_or(now);
        if now < deadline {
            sleep(Duration::from_nanos(deadline - now));
        }
        self.next_deadline = Some(deadline.max(now) + grab.interval_nanos);

        let reply = self
            .conn
            .get_image(
                ImageFormat::Z_PIXMAP,
                grab.drawable,
                grab.x,
                grab.y,
                grab.width,
                grab.height,
                !0, // all planes
            )
            .map_err(be)?
            .reply()
            .map_err(be)?;
        let timestamp = self.clock.now();

        // `start` rejects a zero-area target, so height is non-zero here.
        let height = grab.height as usize;
        let stride = reply.data.len() / height;

        Ok(Frame {
            size: Size::new(grab.width as u32, grab.height as u32),
            stride,
            // X ZPixmap on a little-endian server is BGRA byte order.
            format: PixelFormat::Bgra8,
            timestamp,
            data: reply.data,
        })
    }

    fn stop(&mut self) -> Result<()> {
        self.grab = None;
        self.next_deadline = None;
        Ok(())
    }
}
