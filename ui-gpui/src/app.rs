// Winkel's one screen, in GPUI. The state the backend mirrors, the
// keyboard ring, the dialogs' drafts, the hero editor, the wheels — the
// same fields Main.qml keeps, driven by the same events, pushed down the
// same wire. Rendering lives in render.rs; this file is the behaviour.

use std::time::Instant;

use gpui::{App, Context, FocusHandle, KeyDownEvent, MouseButton, Render, Window};

use crate::font::FontMetrics;
use crate::hero::Hero;
use crate::i18n::{Arg, I18n};
use crate::protocol::{Bridge, Event, Request, StateEvent};
use crate::text::TextCache;
use crate::theme::Theme;
use crate::wheel::Wheel;

pub const WINDOW_W: f64 = 377.0;
pub const WINDOW_H: f64 = 610.0;

// The controls the keyboard ring can name, in the order the QML builds
// them. The ring is rebuilt on every open/close path, so Tab always cycles
// the view in front.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingEntry {
    Beat(u32),
    BpmMinus,
    BpmPlus,
    Play,
    TimeSignature,
    Tap,
    Subdivision,
    NumWheel,
    DenWheel,
    DialogCancel,
    DialogOk,
    SubTile(u32),
    SubSlot(u32),
}

// Which control a pointer is holding down, for the pressed scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pressed {
    Beat(u32),
    BpmMinus,
    BpmPlus,
    Play,
    TimeSignature,
    Tap,
    Subdivision,
    DialogCancel,
    DialogOk,
    SubTile(u32),
    SubSlot(u32),
    Wheel,
    Close,
}

pub struct AppView {
    pub focus: FocusHandle,
    pub hero_focus: FocusHandle,

    pub bridge: Option<Bridge>,
    pub backend_bin: String,
    pub quitting: bool,

    pub theme: Theme,
    pub i18n: I18n,
    pub font: FontMetrics,
    pub texts: TextCache,
    pub figure_cache: Vec<(u32, u32, u32, u32, f64)>, // cache invalidation markers

    // --- the state the backend mirrors ---
    pub loading: bool,
    pub running: bool,
    pub bpm: f64,
    pub beats: u32,
    pub denominator: u32,
    pub voices: Vec<u32>,
    pub current_beat: i32,
    pub failure: String,
    pub silent_output: bool,
    pub tap_times: Vec<f64>,
    pub subdivision: u32,
    pub subpattern: u32,
    pub subshape: u32,
    pub ts_open: bool,
    pub sub_open: bool,
    pub keys_open: bool,

    pub hero: Hero,
    pub focus_index: i32,
    pub tabbing: bool,

    pub ts_draft_beats: u32,
    pub ts_draft_denominator: u32,
    pub sub_draft_division: u32,
    pub sub_draft_mask: u32,
    pub sub_draft_shape: u32,

    pub num_wheel: Wheel,
    pub den_wheel: Wheel,

    // Painted geometry the hit-area builders read back.
    pub wheel_num_top: f64,
    pub wheel_den_top: f64,
    pub wheel_num_x: f64,
    pub wheel_den_x: f64,
    pub sub_tile_rects: Vec<(f64, f64, f64, f64)>,
    pub sub_slot_rects: Vec<(f64, f64, f64, f64)>,

    pub pressed: Option<(Pressed, Instant)>,
    pub hovered_tile: Option<u32>,
    pub hovered_slot: Option<u32>,

    pub save_generation: u64,
    pub tasks: Vec<gpui::Task<()>>,
}

impl AppView {
    pub fn new(focus: FocusHandle, hero_focus: FocusHandle, backend_bin: String) -> Self {
        AppView {
            focus,
            hero_focus,
            bridge: None,
            backend_bin,
            quitting: false,
            theme: Theme::default(),
            i18n: I18n::load(),
            font: FontMetrics::resolve(&std::env::var("WINKEL_FONT").unwrap_or_else(|_| "JetBrainsMono Nerd Font".into()))
                .expect("the theme font must resolve through fontconfig"),
            texts: TextCache::default(),
            figure_cache: Vec::new(),
            loading: true,
            running: false,
            bpm: 80.0,
            beats: 4,
            denominator: 4,
            voices: vec![3, 1, 1, 1, 2, 2, 1, 3, 3, 2, 3, 1, 2, 2, 1, 3],
            current_beat: -1,
            failure: String::new(),
            silent_output: false,
            tap_times: Vec::new(),
            subdivision: 1,
            subpattern: 1,
            subshape: 1,
            ts_open: false,
            sub_open: false,
            keys_open: false,
            hero: Hero::default(),
            focus_index: -1,
            tabbing: false,
            ts_draft_beats: 4,
            ts_draft_denominator: 4,
            sub_draft_division: 1,
            sub_draft_mask: 1,
            sub_draft_shape: 1,
            num_wheel: Wheel::new((1..=16).collect()),
            den_wheel: Wheel::new(vec![1, 2, 4, 8]),
            wheel_num_top: 0.0,
            wheel_den_top: 0.0,
            wheel_num_x: 0.0,
            wheel_den_x: 0.0,
            sub_tile_rects: Vec::new(),
            sub_slot_rects: Vec::new(),
            pressed: None,
            hovered_tile: None,
            hovered_slot: None,
            save_generation: 0,
            tasks: Vec::new(),
        }
    }

    // The first state line arrives before the first paint; until it has
    // been applied, no control pushes its own value back down the wire.
    pub fn apply_state(&mut self, s: &StateEvent, cx: &mut Context<Self>) {
        self.loading = true;
        self.bpm = s.bpm;
        self.beats = s.beats;
        self.denominator = s.denominator;
        self.subdivision = if s.subdivision == 0 { 1 } else { s.subdivision };
        self.subpattern = if s.subpattern == 0 { 1 } else { s.subpattern };
        self.subshape = if s.subshape == 0 { 1 } else { s.subshape };
        if s.voices.len() == 16 {
            self.voices = s.voices.clone();
        }
        self.hero.set(s.bpm.round() as u32);
        self.loading = false;
        cx.notify();
    }

    pub fn handle_event(&mut self, event: Event, cx: &mut Context<Self>) -> bool {
        match event {
            Event::State(s) => self.apply_state(&s, cx),
            Event::Ready { silent, .. } => {
                self.silent_output = silent;
                self.failure.clear();
                cx.notify();
            }
            Event::Started => {
                self.running = true;
                cx.notify();
            }
            Event::Beat { beat, .. } => {
                self.current_beat = beat as i32;
                cx.notify();
            }
            Event::Stopped { .. } => {
                self.running = false;
                self.current_beat = -1;
                cx.notify();
            }
            Event::Error { code, msg, detail } => {
                // The code picks the words; msg is the backend's English,
                // kept for a code this build does not know.
                let key = format!("error.wire.{}", code);
                if !code.is_empty() && self.i18n.has(&key) {
                    self.failure = self.i18n.tr(&key, &[("detail", Arg::Str(detail))]);
                } else if !msg.is_empty() {
                    self.failure = msg;
                } else {
                    self.failure = self.i18n.tr1("error.backendUnknown");
                }
                cx.notify();
            }
            Event::QuitReady => {
                self.quitting = true;
                return false;
            }
        }
        true
    }

    // --- the wire ---

    pub fn send(&self, command: &Request) {
        if let Some(bridge) = &self.bridge {
            bridge.send(command);
        }
    }

    pub fn current_state(&self) -> StateEvent {
        StateEvent {
            bpm: self.bpm,
            beats: self.beats,
            denominator: self.denominator,
            voices: self.voices.clone(),
            volume: 0.8,
            subdivision: self.subdivision,
            subpattern: self.subpattern,
            subshape: self.subshape,
        }
    }

    pub fn push_params(&self) {
        if self.loading {
            return;
        }
        self.send(&Request::Params(self.current_state()));
    }

    pub fn push_save(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        self.save_generation += 1;
        let generation = self.save_generation;
        let state = self.current_state();
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
            let _ = this.update(cx, |view, cx| {
                if view.save_generation == generation && !view.loading {
                    if let Some(bridge) = view.bridge.clone() {
                        bridge.send(&Request::Save(state));
                    }
                }
                cx.notify();
            });
        });
        self.tasks.push(task);
    }

    // The cells push both: the params move the clock, the save persists.
    pub fn set_cell(&mut self, n: u32, mask: u32, shape: Option<u32>, cx: &mut Context<Self>) {
        let cell = crate::rhythm::cell(n, mask, shape);
        let shape = cell.shape & ((1 << n) - 1) | mask | 1;
        let shape = shape & ((1 << n) - 1);
        if n == self.subdivision && mask == self.subpattern && shape == self.subshape {
            return;
        }
        self.subdivision = n;
        self.subpattern = mask;
        self.subshape = shape;
        self.push_params();
        self.push_save(cx);
    }

    pub fn clamp_bpm(v: f64) -> f64 {
        v.clamp(10.0, 400.0)
    }

    pub fn set_bpm(&mut self, v: f64, cx: &mut Context<Self>) {
        let v = Self::clamp_bpm(v.round());
        if v != self.bpm {
            self.bpm = v;
            self.push_params();
            self.push_save(cx);
        }
        self.hero.set(v.round() as u32);
    }

    pub fn cycle_voice(&mut self, i: u32, cx: &mut Context<Self>) {
        let mut next = self.voices.clone();
        next[i as usize] = (next[i as usize] + 1) % 4;
        self.voices = next;
        self.push_params();
        self.push_save(cx);
    }

    pub fn tap(&mut self, cx: &mut Context<Self>) {
        let now = now_ms();
        if let Some(last) = self.tap_times.last() {
            if now - last > 2000.0 {
                self.tap_times.clear();
            }
        }
        self.tap_times.push(now);
        if self.tap_times.len() > 6 {
            self.tap_times.remove(0);
        }
        if self.tap_times.len() >= 2 {
            let mut sum = 0.0;
            for i in 1..self.tap_times.len() {
                sum += self.tap_times[i] - self.tap_times[i - 1];
            }
            let bpm = 60000.0 / (sum / (self.tap_times.len() - 1) as f64);
            self.set_bpm(bpm, cx);
        }
    }

    // --- the dialogs ---

    // Every open/close path rebuilds the ring, so Tab always cycles the
    // view in front.
    pub fn toggle_ts(&mut self, cx: &mut Context<Self>) {
        self.ts_open = !self.ts_open;
        self.focus_index = -1;
        self.tabbing = false;
        if self.ts_open {
            self.ts_draft_beats = self.beats;
            self.ts_draft_denominator = self.denominator;
            self.num_wheel.select(self.ts_draft_beats);
            self.den_wheel.select(self.ts_draft_denominator);
        }
        cx.notify();
    }

    pub fn toggle_sub(&mut self, cx: &mut Context<Self>) {
        self.sub_open = !self.sub_open;
        self.focus_index = -1;
        self.tabbing = false;
        if self.sub_open {
            self.sub_draft_division = self.subdivision;
            self.sub_draft_mask = self.subpattern;
            self.sub_draft_shape = self.subshape;
        }
        cx.notify();
    }

    pub fn close_dialogs(&mut self, cx: &mut Context<Self>) {
        self.ts_open = false;
        self.sub_open = false;
        self.focus_index = -1;
        self.tabbing = false;
        cx.notify();
    }

    // The tiles in catalogue order, for the keyboard ring.
    pub fn sub_tiles(&self) -> Vec<(u32, u32, u32)> {
        let mut out = Vec::new();
        for n in [1u32, 2, 3, 4, 5, 6] {
            for cell in crate::rhythm::tiles_for(n) {
                out.push((cell.n, cell.mask, cell.shape));
            }
        }
        out
    }

    pub fn toggle_slot(&mut self, i: u32, cx: &mut Context<Self>) {
        let starts = crate::rhythm::starts(self.sub_draft_division, self.sub_draft_shape);
        let Some(&slot) = starts.get(i as usize) else { return };
        let next = self.sub_draft_mask ^ (1 << slot);
        if next == 0 {
            return;
        }
        self.sub_draft_mask = next;
        cx.notify();
    }

    pub fn commit_sub(&mut self, cx: &mut Context<Self>) {
        let cell = crate::rhythm::cell(self.sub_draft_division, self.sub_draft_mask, Some(self.sub_draft_shape));
        let division = self.sub_draft_division;
        let mask = self.sub_draft_mask;
        let shape = cell.shape;
        self.set_cell(division, mask, Some(shape), cx);
        self.sub_open = false;
        self.tabbing = false;
        self.focus_index = -1;
        cx.notify();
    }

    pub fn commit_ts(&mut self, cx: &mut Context<Self>) {
        self.beats = self.ts_draft_beats;
        self.denominator = self.ts_draft_denominator;
        self.push_params();
        self.push_save(cx);
        self.ts_open = false;
        self.tabbing = false;
        self.focus_index = -1;
        cx.notify();
    }

    // --- the keyboard ring ---

    pub fn ring(&self) -> Vec<RingEntry> {
        if self.ts_open {
            vec![RingEntry::NumWheel, RingEntry::DenWheel, RingEntry::DialogCancel, RingEntry::DialogOk]
        } else if self.sub_open {
            let mut r: Vec<RingEntry> = self
                .sub_tiles()
                .iter()
                .enumerate()
                .map(|(i, _)| RingEntry::SubTile(i as u32))
                .collect();
            for i in 0..crate::rhythm::starts(self.sub_draft_division, self.sub_draft_shape).len() {
                r.push(RingEntry::SubSlot(i as u32));
            }
            r.push(RingEntry::DialogCancel);
            r.push(RingEntry::DialogOk);
            r
        } else {
            let mut r: Vec<RingEntry> = (0..self.beats).map(RingEntry::Beat).collect();
            r.extend([RingEntry::BpmMinus, RingEntry::BpmPlus, RingEntry::Play, RingEntry::TimeSignature, RingEntry::Tap, RingEntry::Subdivision]);
            r
        }
    }

    pub fn cycle_focus(&mut self, back: bool, cx: &mut Context<Self>) {
        self.tabbing = true;
        let n = self.ring().len() as i32;
        if n == 0 {
            self.focus_index = -1;
            cx.notify();
            return;
        }
        let mut i = self.focus_index + if back { -1 } else { 1 };
        if i >= n {
            i = -1;
        }
        if i < -1 {
            i = n - 1;
        }
        self.focus_index = i;
        cx.notify();
    }

    pub fn activate_focus(&mut self, cx: &mut Context<Self>) {
        let entry = self.focused_entry();
        if let Some(entry) = entry {
            self.activate(entry, cx);
        }
    }

    fn focused_entry(&self) -> Option<RingEntry> {
        if self.focus_index >= 0 {
            self.ring().get(self.focus_index as usize).copied()
        } else {
            None
        }
    }

    pub fn activate(&mut self, entry: RingEntry, cx: &mut Context<Self>) {
        match entry {
            RingEntry::Beat(i) => self.cycle_voice(i, cx),
            RingEntry::BpmMinus => self.set_bpm(self.bpm - 1.0, cx),
            RingEntry::BpmPlus => self.set_bpm(self.bpm + 1.0, cx),
            RingEntry::Play => {
                self.send(&Request::Toggle);
            }
            RingEntry::TimeSignature => self.toggle_ts(cx),
            RingEntry::Tap => self.tap(cx),
            RingEntry::Subdivision => self.toggle_sub(cx),
            RingEntry::NumWheel => {}
            RingEntry::DenWheel => {}
            RingEntry::DialogCancel => {
                self.ts_open = false;
                self.sub_open = false;
                self.tabbing = false;
                self.focus_index = -1;
                cx.notify();
            }
            RingEntry::DialogOk => {
                if self.ts_open {
                    self.commit_ts(cx);
                } else {
                    self.commit_sub(cx);
                }
            }
            RingEntry::SubTile(i) => {
                if let Some(&(n, mask, shape)) = self.sub_tiles().get(i as usize) {
                    self.sub_draft_division = n;
                    self.sub_draft_mask = mask;
                    self.sub_draft_shape = shape;
                    cx.notify();
                }
            }
            RingEntry::SubSlot(i) => self.toggle_slot(i, cx),
        }
    }

    // The stepped entries: arrows walk the catalogue or the tempo.
    pub fn step_focus(&mut self, d: i32, cx: &mut Context<Self>) {
        let Some(entry) = self.focused_entry() else { return };
        match entry {
            RingEntry::BpmMinus | RingEntry::BpmPlus => self.set_bpm(self.bpm + d as f64, cx),
            RingEntry::NumWheel => {
                let next = (self.ts_draft_beats as i32 + d).clamp(1, 16) as u32;
                self.ts_draft_beats = next;
                self.num_wheel.select(next);
                cx.notify();
            }
            RingEntry::DenWheel => {
                let ladder = [1u32, 2, 4, 8];
                let i = ladder.iter().position(|v| *v == self.ts_draft_denominator).unwrap_or(0);
                let next = ladder[(i as i32 + d).clamp(0, 3) as usize];
                self.ts_draft_denominator = next;
                self.den_wheel.select(next);
                cx.notify();
            }
            RingEntry::SubTile(_) => {
                let cells = self.sub_tiles();
                let at = cells
                    .iter()
                    .position(|&(n, mask, _)| n == self.sub_draft_division && mask == self.sub_draft_mask)
                    .unwrap_or(0);
                let next = (at as i32 + d).clamp(0, cells.len() as i32 - 1) as usize;
                let (n, mask, shape) = cells[next];
                self.sub_draft_division = n;
                self.sub_draft_mask = mask;
                self.sub_draft_shape = shape;
                cx.notify();
            }
            _ => {}
        }
    }

    // Taps anywhere keep the ring in step with the mouse: the tap is
    // hit-tested against the ring's own controls.
    pub fn sync_focus_to_point(&mut self, _x: f64, _y: f64, cx: &mut Context<Self>) {
        self.tabbing = false;
        // Hit-testing the ring by point is the QML's syncFocusToPoint; the
        // gpui tree resolves clicks to controls directly, so a tap only
        // needs to clear the frame.
        self.focus_index = -1;
        cx.notify();
    }

    // --- the close, a drain not a cut ---

    pub fn request_quit(&mut self, cx: &mut Context<Self>) {
        if self.quitting {
            return;
        }
        // Flush a pending save before the drain, as flushSave does.
        if self.save_generation > 0 && !self.loading {
            self.save_generation += 1;
            self.send(&Request::Save(self.current_state()));
        }
        self.quitting = true;
        match &self.bridge {
            Some(_) => self.send(&Request::Quit),
            None => cx.quit(),
        }
    }
}

pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

// The main view's keyboard, the first-class input. One handler mirrors the
// QML's Keys.onPressed, dialogs included.
pub fn handle_key(view: &mut AppView, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<AppView>) {
    use gpui::Modifiers;

    let keystroke = &event.keystroke;
    let key = keystroke.key.as_str();
    let mods = keystroke.modifiers;

    // The keys sheet owns the keyboard while it is open.
    if view.keys_open {
        if key == "escape" || key == "?" {
            view.keys_open = false;
        }
        cx.notify();
        return;
    }

    // A dialog owns the keyboard while it is open.
    if view.ts_open || view.sub_open {
        match key {
            "tab" => view.cycle_focus(mods.shift, cx),
            "escape" => view.close_dialogs(cx),
            "enter" | "space" => view.activate_focus(cx),
            "up" | "right" => view.step_focus(1, cx),
            "down" | "left" => view.step_focus(-1, cx),
            _ => {}
        }
        cx.notify();
        return;
    }

    // The hero owns its keys while it has the focus.
    if view.hero_focus.is_focused(window) {
        if hero_key(view, key, mods, window, cx) {
            cx.notify();
            return;
        }
        // Anything the hero does not take falls through to the view, the
        // way an unaccepted key bubbles in the QML.
    }

    // A focused control answers enter — and only while tab navigation is
    // in play, the way the QML gates it.
    if key == "enter" && view.tabbing {
        view.activate_focus(cx);
        cx.notify();
        return;
    }

    match key {
        "tab" => view.cycle_focus(mods.shift, cx),
        "?" => {
            view.keys_open = true;
            view.tabbing = false;
        }
        "space" => {
            view.send(&Request::Toggle);
            view.focus_index = view.ring().iter().position(|e| *e == RingEntry::Play).map(|i| i as i32).unwrap_or(-1);
            view.tabbing = false;
        }
        "t" => {
            view.tap(cx);
            view.focus_index = view.ring().iter().position(|e| *e == RingEntry::Tap).map(|i| i as i32).unwrap_or(-1);
            view.tabbing = false;
        }
        "up" => view.set_bpm(view.bpm + if mods.shift { 5.0 } else { 1.0 }, cx),
        "down" => view.set_bpm(view.bpm - if mods.shift { 5.0 } else { 1.0 }, cx),
        "pageup" => view.set_bpm(view.bpm + 10.0, cx),
        "pagedown" => view.set_bpm(view.bpm - 10.0, cx),
        "escape" => {
            if view.running {
                view.send(&Request::Stop);
            }
        }
        "q" if mods.control => view.request_quit(cx),
        "1" | "2" | "4" | "8" => {
            view.denominator = key.parse().unwrap();
            view.push_params();
            view.push_save(cx);
            view.focus_index = view.ring().iter().position(|e| *e == RingEntry::TimeSignature).map(|i| i as i32).unwrap_or(-1);
            view.tabbing = false;
        }
        _ => {}
    }
    cx.notify();
}

// The hero's own keys; true when the hero took the key.
fn hero_key(view: &mut AppView, key: &str, mods: gpui::Modifiers, window: &mut Window, cx: &mut Context<AppView>) -> bool {
    let shift = mods.shift;
    match key {
        "up" => {
            let bpm = view.bpm + if shift { 5.0 } else { 1.0 };
            view.set_bpm(bpm, cx);
            true
        }
        "down" => {
            let bpm = view.bpm - if shift { 5.0 } else { 1.0 };
            view.set_bpm(bpm, cx);
            true
        }
        "enter" => {
            // Commit and hand the focus back to the view.
            window.blur();
            hero_editing_finished(view, cx);
            true
        }
        "escape" => {
            view.hero.set(view.bpm.round() as u32);
            window.blur();
            true
        }
        "backspace" => {
            view.hero.backspace();
            true
        }
        "delete" => {
            view.hero.delete();
            true
        }
        "left" => {
            view.hero.move_cursor(true, shift);
            true
        }
        "right" => {
            view.hero.move_cursor(false, shift);
            true
        }
        "home" => {
            view.hero.home(shift);
            true
        }
        "end" => {
            view.hero.end(shift);
            true
        }
        "a" if mods.control => {
            view.hero.select_all();
            true
        }
        "v" if mods.control => {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                view.hero.paste(&text);
            }
            true
        }
        "c" if mods.control => {
            let text = view.hero.selected_text();
            if !text.is_empty() {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
            true
        }
        "x" if mods.control => {
            let text = view.hero.cut();
            if !text.is_empty() {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
            true
        }
        k if k.len() == 1 && k.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
            view.hero.insert(k);
            true
        }
        _ => false,
    }
}

fn hero_editing_finished(view: &mut AppView, cx: &mut Context<AppView>) {
    match view.hero.parse() {
        Some(v) => view.set_bpm(v as f64, cx),
        None => view.hero.set(view.bpm.round() as u32),
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        crate::render::render_app(self, window, cx)
    }
}
