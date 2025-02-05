use std::cell::Cell;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use log::debug;

/// A clock that ticks in sync with the provided frame rate.
#[derive(Debug)]
pub struct FpsClock {
    last_tick_time: Cell<Instant>,
    fps_in_nanos: f32,
}

impl FpsClock {
    pub fn new(fps: u32) -> FpsClock {
        FpsClock {
            last_tick_time: Cell::new(Instant::now()),
            fps_in_nanos: (1.0 / fps as f32) * 1_000_000_000.,
        }
    }

    pub fn tick(&self) {
        let t = self.last_tick_time.get().elapsed();
        let total_nanos = t.as_secs() * 1_000_000_000 + t.subsec_nanos() as u64;
        let diff = self.fps_in_nanos - (total_nanos as f32);
        if diff > 0.0 {
            thread::sleep(Duration::new(0, diff as u32))
        } else {
            debug!(target: "context", "ticking running late at {diff:?}");
        }
        self.last_tick_time.set(Instant::now());
    }
}
