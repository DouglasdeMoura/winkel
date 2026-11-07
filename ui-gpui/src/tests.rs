// Feature tests: the real AppView in a real gpui window, driven through the
// same dispatch path the compositor feeds — window.dispatch_keystroke, the
// public door into gpui's key dispatch. These are the tests the live session
// cannot give deterministically: the keys sheet, the dialogs, the tab ring,
// the hero editor, and the shortcuts the help sheet promises.

use gpui::prelude::*;
use gpui::{Keystroke, TestAppContext, Window, WindowHandle};

use crate::app::AppView;

fn press(window: &mut Window, cx: &mut gpui::Context<crate::app::AppView>, key: &str) {
    window.dispatch_keystroke(
        Keystroke {
            key: key.into(),
            ..Default::default()
        },
        cx,
    );
}

fn spawn(cx: &mut TestAppContext) -> WindowHandle<AppView> {
    let handle = cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle();
                focus.focus(window);
                let hero_focus = cx.focus_handle();
                let mut view = AppView::new(focus, hero_focus, "winkel".into());
                view.bridge = None; // offline: sends become no-ops
                view
            })
        })
        .unwrap()
    });
    cx.run_until_parked();
    handle
}

fn read<R>(cx: &mut TestAppContext, win: &WindowHandle<AppView>, f: impl FnOnce(&AppView) -> R + Send + 'static) -> R
where
    R: Send + 'static,
{
    cx.update(|cx| win.update(cx, |view: &mut AppView, _window, _cx| f(view)).unwrap())
}

fn hit(cx: &mut TestAppContext, win: &WindowHandle<AppView>, key: &'static str) {
    // The harness's own simulator: it dispatches through the same key path
    // without nesting an entity borrow inside the dispatch.
    cx.simulate_keystrokes((*win).into(), key);
}

#[gpui::test]
async fn question_opens_the_keys_sheet(cx: &mut TestAppContext) {
    let win = spawn(cx);
    assert!(!read(cx, &win, |v| v.keys_open));
    hit(cx, &win, "?");
    assert!(read(cx, &win, |v| v.keys_open));
    // Escape — the sheet owns the keyboard while it is open.
    hit(cx, &win, "escape");
    assert!(!read(cx, &win, |v| v.keys_open));
}

#[gpui::test]
async fn arrows_step_the_tempo_and_clamp(cx: &mut TestAppContext) {
    let win = spawn(cx);
    for _ in 0..3 {
        hit(cx, &win, "up");
    }
    assert_eq!(read(cx, &win, |v| v.bpm), 83.0);
    // Shift-up steps by five.
    hit(cx, &win, "shift-up");
    assert_eq!(read(cx, &win, |v| v.bpm), 88.0);
    for _ in 0..200 {
        hit(cx, &win, "down");
    }
    assert_eq!(read(cx, &win, |v| v.bpm), 10.0, "the floor is 10 bpm");
}

#[gpui::test]
async fn beat_units_set_the_denominator(cx: &mut TestAppContext) {
    let win = spawn(cx);
    hit(cx, &win, "8");
    assert_eq!(read(cx, &win, |v| v.denominator), 8);
    // The ring follows the meter: four beats plus six controls.
    assert_eq!(read(cx, &win, |v| v.ring().len()), 10);
}

#[gpui::test]
async fn tab_walks_the_ring_and_enter_clicks(cx: &mut TestAppContext) {
    let win = spawn(cx);
    // Tab: the focus frame appears, the ring starts at beat 1.
    hit(cx, &win, "tab");
    assert!(read(cx, &win, |v| v.tabbing));
    assert_eq!(read(cx, &win, |v| v.focus_index), 0);
    // The ring's first four entries are the beats.
    assert!(matches!(read(cx, &win, |v| v.ring()[0]), crate::app::RingEntry::Beat(0)));
    // Walk to the time-signature button: beats(4) minus plus play ts → index 7.
    for _ in 0..7 {
        hit(cx, &win, "tab");
    }
    assert_eq!(read(cx, &win, |v| v.focus_index), 7);
    // Enter opens the time-signature editor.
    hit(cx, &win, "enter");
    assert!(read(cx, &win, |v| v.ts_open));
    // Inside the dialog the ring restarts: wheel, wheel, cancel, ok.
    assert_eq!(read(cx, &win, |v| v.ring().len()), 4);
    // Escape throws the draft away.
    hit(cx, &win, "escape");
    assert!(!read(cx, &win, |v| v.ts_open));
}

#[gpui::test]
async fn the_time_signature_dialog_commits_its_draft(cx: &mut TestAppContext) {
    let win = spawn(cx);
    // Open the editor the way the ring does: walk to the button, press it.
    for _ in 0..8 {
        hit(cx, &win, "tab");
    }
    hit(cx, &win, "enter");
    assert!(read(cx, &win, |v| v.ts_open));
    assert_eq!(read(cx, &win, |v| v.ts_draft_beats), 4);

    // The dialog owns the keyboard: tab focuses the numerator wheel and
    // right steps it 4 → 5.
    hit(cx, &win, "tab");
    hit(cx, &win, "right");
    assert_eq!(read(cx, &win, |v| v.ts_draft_beats), 5);

    // Tab to the denominator wheel and step 4 → 8.
    hit(cx, &win, "tab");
    hit(cx, &win, "right");
    assert_eq!(read(cx, &win, |v| v.ts_draft_denominator), 8);

    // OK commits: tab twice lands on the primary, enter presses it.
    hit(cx, &win, "tab");
    hit(cx, &win, "tab");
    hit(cx, &win, "enter");
    assert!(!read(cx, &win, |v| v.ts_open));
    assert_eq!(read(cx, &win, |v| v.beats), 5);
    assert_eq!(read(cx, &win, |v| v.denominator), 8);
}
