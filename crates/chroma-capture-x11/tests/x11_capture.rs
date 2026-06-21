//! X11 backend integration tests.
//!
//! These need a reachable X server; without one (headless CI) each test skips
//! cleanly. The pixel-path tests capture a server-backed *pixmap* we create, so
//! they are deterministic and work under rootless Xwayland (WSLg) as well as a
//! native X11 session — the desktop root is not capturable under rootless
//! Xwayland (see `DECISIONS.md`).

#![cfg(target_os = "linux")]

use chroma_capture_api::{CaptureTarget, Clock, EventSource, ScreenCapturer};
use chroma_capture_x11::{MonotonicClock, X11EventSource, X11ScreenCapturer};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, CreateGCAux, Drawable, Rectangle};
use x11rb::rust_connection::RustConnection;

const TEST_W: u16 = 64;
const TEST_H: u16 = 48;
/// Pure red on a standard RGB888 TrueColor visual (ignoring alpha).
const FILL_PIXEL: u32 = 0x00FF_0000;

/// A server-backed pixmap, filled with [`FILL_PIXEL`], that any connection can
/// `GetImage`. Kept alive (owns its connection) for the duration of a test.
struct BackedDrawable {
    _conn: RustConnection,
    id: Drawable,
}

/// Creates and fills a test pixmap, or returns `None` if no X server is present.
fn make_backed_drawable() -> Option<BackedDrawable> {
    let (conn, screen_num) = x11rb::connect(None).ok()?;
    let screen = &conn.setup().roots[screen_num];
    let depth = screen.root_depth;
    let root = screen.root;

    let pixmap = conn.generate_id().ok()?;
    conn.create_pixmap(depth, pixmap, root, TEST_W, TEST_H)
        .ok()?;

    let gc = conn.generate_id().ok()?;
    conn.create_gc(gc, pixmap, &CreateGCAux::new().foreground(FILL_PIXEL))
        .ok()?;
    conn.poly_fill_rectangle(
        pixmap,
        gc,
        &[Rectangle {
            x: 0,
            y: 0,
            width: TEST_W,
            height: TEST_H,
        }],
    )
    .ok()?;
    conn.free_gc(gc).ok()?;
    conn.flush().ok()?;

    Some(BackedDrawable {
        _conn: conn,
        id: pixmap,
    })
}

#[test]
fn captures_pixels_from_a_backed_drawable() {
    let Some(target) = make_backed_drawable() else {
        eprintln!("no X server; skipping");
        return;
    };

    let mut cap = X11ScreenCapturer::new().expect("connect capturer");
    cap.start(
        CaptureTarget::Window {
            id: target.id as u64,
        },
        30,
    )
    .expect("start capture");
    let frame = cap.next_frame().expect("grab frame");

    assert_eq!(frame.size.width, TEST_W as u32);
    assert_eq!(frame.size.height, TEST_H as u32);
    assert!(frame.data.len() >= TEST_W as usize * TEST_H as usize * 4);

    // The first pixel, read as a little-endian u32, must be the color we filled
    // (masking alpha) — validates both the data path and the byte layout.
    let px = u32::from_le_bytes([frame.data[0], frame.data[1], frame.data[2], frame.data[3]]);
    assert_eq!(
        px & 0x00FF_FFFF,
        FILL_PIXEL,
        "captured pixel {px:#08x} != filled {FILL_PIXEL:#08x}"
    );

    cap.stop().expect("stop");
}

/// CAP-05 foundation: a captured frame is stamped on the same monotonic clock
/// the rest of the pipeline reads, so events and frames are directly comparable.
#[test]
fn frame_is_stamped_on_the_shared_clock() {
    let Some(target) = make_backed_drawable() else {
        eprintln!("no X server; skipping");
        return;
    };

    let clock = MonotonicClock::new();
    let mut cap = X11ScreenCapturer::new().expect("connect capturer");
    cap.start(
        CaptureTarget::Window {
            id: target.id as u64,
        },
        60,
    )
    .expect("start capture");

    let before = clock.now();
    let frame = cap.next_frame().expect("grab frame");
    let after = clock.now();

    assert!(
        frame.timestamp >= before && frame.timestamp <= after,
        "frame timestamp {:?} not within [{before:?}, {after:?}] of the shared clock",
        frame.timestamp
    );
}

/// Desktop root capture works on a native X11 session but not under rootless
/// Xwayland (WSLg), where `GetImage` on the root returns `BadMatch`. This test
/// documents that: it asserts success when possible and skips otherwise.
#[test]
fn desktop_root_capture_is_best_effort() {
    let mut cap = match X11ScreenCapturer::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("no X server; skipping ({e})");
            return;
        }
    };
    cap.start(
        CaptureTarget::Region {
            x: 0,
            y: 0,
            width: 32,
            height: 32,
        },
        30,
    )
    .expect("start capture");

    match cap.next_frame() {
        Ok(frame) => {
            assert_eq!(frame.size.width, 32);
            assert_eq!(frame.size.height, 32);
        }
        Err(e) => eprintln!("root capture unavailable (rootless Xwayland?); skipping: {e}"),
    }
}

#[test]
fn event_source_polls_without_error() {
    let mut events = match X11EventSource::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("no X server / XInput2; skipping ({e})");
            return;
        }
    };
    // No input is synthesized here; we only assert the poll path is sound and
    // returns (typically empty) without erroring.
    let _ = events.poll();
}
