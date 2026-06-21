//! Test doubles for the capture contract.
//!
//! `chroma-project` and the M1 record path can be exercised against these with
//! no real X11/Wayland device present (`ORCHESTRATION.md` §10): a deterministic
//! clock, a capturer that emits synthetic frames, and a scripted event source.

use crate::capturer::{CaptureTarget, ScreenCapturer};
use crate::clock::Clock;
use crate::error::Result;
use crate::event::{EventSource, TimedInputEvent};
use crate::frame::{Frame, PixelFormat};
use chroma_core_api::{Size, TimeStamp};
use std::cell::Cell;

/// A clock that advances by a fixed step on every [`now`](Clock::now) call,
/// making capture timing fully deterministic for tests.
pub struct FakeClock {
    step_nanos: u64,
    next: Cell<u64>,
}

impl FakeClock {
    /// Creates a clock that starts at zero and advances `step_nanos` per read.
    pub fn new(step_nanos: u64) -> Self {
        FakeClock {
            step_nanos,
            next: Cell::new(0),
        }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> TimeStamp {
        let t = self.next.get();
        self.next.set(t + self.step_nanos);
        TimeStamp::from_nanos(t)
    }
}

/// A capturer that emits a fixed number of solid-color frames, then errors with
/// [`crate::CaptureError::Timeout`] to signal end-of-stream.
pub struct FakeScreenCapturer {
    size: Size,
    frame_interval_nanos: u64,
    remaining: u32,
    next_ts: u64,
    started: bool,
}

impl FakeScreenCapturer {
    /// Creates a capturer that will yield `frames` frames of `size`, each
    /// `frame_interval_nanos` apart on the timebase.
    pub fn new(size: Size, frames: u32, frame_interval_nanos: u64) -> Self {
        FakeScreenCapturer {
            size,
            frame_interval_nanos,
            remaining: frames,
            next_ts: 0,
            started: false,
        }
    }
}

impl ScreenCapturer for FakeScreenCapturer {
    fn start(&mut self, _target: CaptureTarget, _fps: u32) -> Result<()> {
        self.started = true;
        Ok(())
    }

    fn next_frame(&mut self) -> Result<Frame> {
        if self.remaining == 0 {
            return Err(crate::error::CaptureError::Timeout);
        }
        self.remaining -= 1;
        let ts = self.next_ts;
        self.next_ts += self.frame_interval_nanos;
        let stride = self.size.width as usize * PixelFormat::Rgba8.bytes_per_pixel();
        Ok(Frame {
            size: self.size,
            stride,
            format: PixelFormat::Rgba8,
            timestamp: TimeStamp::from_nanos(ts),
            data: vec![0u8; stride * self.size.height as usize],
        })
    }

    fn stop(&mut self) -> Result<()> {
        self.started = false;
        Ok(())
    }
}

/// An event source that replays a scripted list of events once.
#[derive(Default)]
pub struct FakeEventSource {
    queued: Vec<TimedInputEvent>,
}

impl FakeEventSource {
    /// Creates a source that will return `events` on its first
    /// [`poll`](EventSource::poll) and nothing thereafter.
    pub fn new(events: Vec<TimedInputEvent>) -> Self {
        FakeEventSource { queued: events }
    }
}

impl EventSource for FakeEventSource {
    fn poll(&mut self) -> Vec<TimedInputEvent> {
        std::mem::take(&mut self.queued)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::InputEvent;

    #[test]
    fn fake_clock_is_monotonic_and_deterministic() {
        let clock = FakeClock::new(100);
        assert_eq!(clock.now(), TimeStamp::from_nanos(0));
        assert_eq!(clock.now(), TimeStamp::from_nanos(100));
        assert_eq!(clock.now(), TimeStamp::from_nanos(200));
    }

    #[test]
    fn fake_capturer_yields_then_ends() {
        let mut cap = FakeScreenCapturer::new(Size::new(4, 2), 2, 16_000_000);
        cap.start(CaptureTarget::FullScreen { monitor: 0 }, 60)
            .unwrap();
        let f0 = cap.next_frame().unwrap();
        assert_eq!(f0.timestamp, TimeStamp::from_nanos(0));
        assert_eq!(f0.data.len(), 4 * 4 * 2);
        let f1 = cap.next_frame().unwrap();
        assert_eq!(f1.timestamp, TimeStamp::from_nanos(16_000_000));
        assert!(cap.next_frame().is_err());
    }

    #[test]
    fn fake_event_source_drains_once() {
        let ev = TimedInputEvent {
            timestamp: TimeStamp::from_nanos(5),
            event: InputEvent::PointerMove { x: 1.0, y: 2.0 },
        };
        let mut src = FakeEventSource::new(vec![ev]);
        assert_eq!(src.poll().len(), 1);
        assert!(src.poll().is_empty());
    }
}
