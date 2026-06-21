//! `record` — a minimal launch demo for the X11 capture backend.
//!
//! Opens a capture session, grabs frames while polling input events, then prints
//! a summary. On Windows (or when no X server is reachable) it reports that
//! capture is unavailable and exits cleanly — so it is safe to run anywhere.
//!
//! Usage:
//! ```text
//! cargo run --example record -- [fullscreen | region | window:<id>] [fps] [frames]
//! ```
//! Defaults: `fullscreen`, 30 fps, `2 * fps` frames.

use chroma_capture_api::{CaptureTarget, Clock, EventSource, ScreenCapturer};
use chroma_capture_x11::open_session;

fn main() {
    let mut args = std::env::args().skip(1);
    let target = match args.next().as_deref() {
        None | Some("fullscreen") => CaptureTarget::FullScreen { monitor: 0 },
        Some("region") => CaptureTarget::Region {
            x: 0,
            y: 0,
            width: 640,
            height: 480,
        },
        Some(other) if other.starts_with("window:") => {
            match other["window:".len()..].parse::<u64>() {
                Ok(id) => CaptureTarget::Window { id },
                Err(_) => {
                    eprintln!("invalid window id: {other}");
                    return;
                }
            }
        }
        Some(other) => {
            eprintln!("unknown target '{other}' (use fullscreen | region | window:<id>)");
            return;
        }
    };
    let fps: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(30);
    let frames: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(fps * 2);

    let mut session = match open_session() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("(expected on Windows — the X11 backend is Linux-only)");
            return;
        }
    };

    println!("recording {frames} frames at {fps} fps from {target:?} — move the mouse / type to generate events");
    if let Err(e) = session.capturer.start(target, fps) {
        eprintln!("failed to start capture: {e}");
        return;
    }

    let start = session.clock.now();
    let mut captured = 0u32;
    let mut total_events = 0usize;
    let mut first_logged = false;

    for _ in 0..frames {
        match session.capturer.next_frame() {
            Ok(frame) => {
                captured += 1;
                if !first_logged {
                    println!(
                        "first frame: {}x{} stride={} format={:?} @ {:?}",
                        frame.size.width,
                        frame.size.height,
                        frame.stride,
                        frame.format,
                        frame.timestamp
                    );
                    first_logged = true;
                }
            }
            Err(e) => {
                eprintln!("frame grab failed: {e}");
                eprintln!("(under rootless Xwayland/WSLg the desktop root isn't capturable — try `window:<id>` of an X11 window, or run on a native X11 session)");
                break;
            }
        }
        total_events += session.events.poll().len();
    }

    let _ = session.capturer.stop();
    let elapsed = session
        .clock
        .now()
        .as_nanos()
        .saturating_sub(start.as_nanos());
    let secs = elapsed as f64 / 1e9;
    let effective = if secs > 0.0 {
        captured as f64 / secs
    } else {
        0.0
    };
    println!("done: {captured} frames, {total_events} input events in {secs:.2}s ({effective:.1} fps effective)");
}
