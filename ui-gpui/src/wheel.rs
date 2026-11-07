// One wheel of the time-signature picker, WheelColumn.qml's behaviour:
// drag it, scroll it, or click a value; whatever rests in the middle band
// is the pick. The wheel is a list of values at fixed 30-px cells; the
// offset places the content in the viewport, and a settle animation snaps
// the nearest value into the middle band, the way StrictlyEnforceRange
// and SnapToItem do together in the QML.

use std::time::{Duration, Instant};

pub const CELL: f64 = 30.0;
pub const WHEEL_HEIGHT: f64 = 150.0;
pub const WHEEL_WIDTH: f64 = 64.0;
pub const BAND_TOP: f64 = (WHEEL_HEIGHT - CELL) / 2.0; // 60

#[derive(Clone, Copy)]
struct Anim {
    from: f64,
    to: f64,
    start: Instant,
}

#[derive(Default)]
pub struct Wheel {
    pub values: Vec<u32>,
    pub offset: f64, // content y at the viewport's top
    drag: Option<(f64, f64)>, // start y, start offset
    anim: Option<Anim>,
}

impl Wheel {
    pub fn new(values: Vec<u32>) -> Self {
        Wheel { values, ..Default::default() }
    }

    // The value resting in the middle band.
    pub fn picked(&self) -> u32 {
        self.values[self.picked_ix().min(self.values.len() - 1)]
    }

    pub fn picked_ix(&self) -> usize {
        let raw = ((self.offset + BAND_TOP) / CELL).round();
        (raw.max(0.0) as usize).min(self.values.len().saturating_sub(1))
    }

    // Spin the wheel so `value` rests in the middle.
    pub fn select(&mut self, value: u32) {
        let Some(i) = self.values.iter().position(|v| *v == value) else { return };
        self.select_ix(i);
    }

    pub fn select_ix(&mut self, i: usize) {
        let target = i as f64 * CELL - BAND_TOP;
        self.offset = target;
        self.anim = None;
        self.drag = None;
    }

    pub fn begin_drag(&mut self, y: f64) {
        self.anim = None;
        self.drag = Some((y, self.offset));
    }

    // Returns true when the pick changed.
    pub fn drag_to(&mut self, y: f64) -> bool {
        let Some((start_y, start_offset)) = self.drag else { return false };
        let before = self.picked_ix();
        self.offset = start_offset - (y - start_y);
        // The range is strictly enforced: the ends never leave the band.
        let max_offset = (self.values.len() as f64 - 1.0) * CELL - BAND_TOP;
        self.offset = self.offset.clamp(-BAND_TOP, max_offset.max(-BAND_TOP));
        self.picked_ix() != before
    }

    pub fn end_drag(&mut self, now: Instant) {
        if self.drag.take().is_some() {
            self.snap(now);
        }
    }

    pub fn scroll(&mut self, steps: f64, now: Instant) {
        let i = self.picked_ix() as f64 - steps;
        let i = (i.max(0.0) as usize).min(self.values.len().saturating_sub(1));
        self.animate_to_ix(i, now);
    }

    pub fn snap(&mut self, now: Instant) {
        self.animate_to_ix(self.picked_ix(), now);
    }

    fn animate_to_ix(&mut self, i: usize, now: Instant) {
        let to = i as f64 * CELL - BAND_TOP;
        if (to - self.offset).abs() < 0.01 {
            self.offset = to;
            self.anim = None;
        } else {
            self.anim = Some(Anim { from: self.offset, to, start: now });
        }
    }

    // Advance a running settle animation; returns true while it runs, so
    // the caller knows to keep frames coming.
    pub fn tick(&mut self, now: Instant) -> bool {
        let Some(anim) = self.anim else { return false };
        let t = (now - anim.start).as_secs_f64() / 0.15;
        if t >= 1.0 {
            self.offset = anim.to;
            self.anim = None;
            return false;
        }
        // OutQuad, the easing the shell's own snap uses.
        self.offset = anim.from + (anim.to - anim.from) * (1.0 - (1.0 - t) * (1.0 - t));
        true
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some() || self.anim.is_some()
    }
}

pub const SNAP_DURATION: Duration = Duration::from_millis(150);
