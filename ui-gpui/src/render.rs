// The painter. Every visual on the window — rects, figures, text — is
// drawn from the frame the render builds: quads first, then paths, then
// text. Interactivity lives on transparent divs layered above, and the
// dialogs paint after the main view, so an open card covers it. Colors
// arrive pre-blended to opaque (in sRGB, the way Qt composites) wherever
// they sit on a solid ground; only the dialog scrim keeps its alpha,
// because its ground is whatever the window shows.

use std::time::Instant;

use gpui::prelude::*;
use gpui::{
    canvas, div, point, px, quad, AnyElement, App, Bounds, Context, Corners, ElementId, Edges,
    ClickEvent, Hsla, MouseButton, Pixels, Rgba, SharedString, Size, Window,
};

use crate::app::{AppView, Pressed, WINDOW_H, WINDOW_W};
use crate::layout::{main_layout, Rect, MainLayout};
use crate::note_figure::{self, FigureGeom};
use crate::rhythm;
use crate::text::{paint_wrapped, qt_line, QtLine, TextCache};
use crate::theme::{ColorRole, Rgba as TRgba, Theme};
use crate::wheel::{Wheel, BAND_TOP, CELL, WHEEL_HEIGHT, WHEEL_WIDTH};

// --- the frame: everything one paint pass draws ---

#[derive(Clone)]
struct FQuad {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: Rgba,
    radius: f32,
    border: Option<(f32, Rgba)>,
}

#[derive(Clone)]
enum FDraw {
    Quad(FQuad),
    Path(gpui::Path<Pixels>, Hsla),
}

#[derive(Clone)]
struct FText {
    line: gpui::ShapedLine,
    x: f64,
    baseline: f64,
}

#[derive(Clone)]
struct FWrapped {
    line: gpui::WrappedLine,
    x: f64,
    baseline: f64,
    line_h: f64,
}

#[derive(Default)]
struct Frame {
    draws: Vec<FDraw>,
    texts: Vec<FText>,
    wrapped: Vec<FWrapped>,
}

impl Frame {
    fn quad(&mut self, x: f64, y: f64, w: f64, h: f64, color: Rgba) {
        if w > 0.0 && h > 0.0 {
            self.draws.push(FDraw::Quad(FQuad { x, y, w, h, color, radius: 0.0, border: None }));
        }
    }

    fn quad_radius(&mut self, x: f64, y: f64, w: f64, h: f64, color: Rgba, radius: f32) {
        if w > 0.0 && h > 0.0 {
            self.draws.push(FDraw::Quad(FQuad { x, y, w, h, color, radius, border: None }));
        }
    }

    fn framed(&mut self, x: f64, y: f64, w: f64, h: f64, color: Rgba, radius: f32, bw: f32, bc: Rgba) {
        if w > 0.0 && h > 0.0 {
            self.draws.push(FDraw::Quad(FQuad { x, y, w, h, color, radius, border: Some((bw, bc)) }));
        }
    }

    fn path(&mut self, path: gpui::Path<Pixels>, color: Hsla) {
        self.draws.push(FDraw::Path(path, color));
    }

    fn text(&mut self, line: gpui::ShapedLine, x: f64, baseline: f64) {
        self.texts.push(FText { line, x, baseline });
    }

    fn text_centered(&mut self, line: gpui::ShapedLine, center_x: f64, baseline: f64) {
        let w = line.width.to_f64();
        self.text(line, center_x - w / 2.0, baseline);
    }

    fn wrapped(&mut self, line: gpui::WrappedLine, x: f64, baseline: f64, line_h: f64) {
        self.wrapped.push(FWrapped { line, x, baseline, line_h });
    }
}

fn color(c: TRgba) -> Rgba {
    Rgba { r: c.rgb.r as f32, g: c.rgb.g as f32, b: c.rgb.b as f32, a: c.a as f32 }
}

fn hsla(c: TRgba) -> Hsla {
    color(c).into()
}

// Qt composites translucency in sRGB; gpui's sRGB framebuffer blends in
// linear. Every translucent token here sits on a known solid ground, so
// the blend is done here — exactly the arithmetic Qt does at render — and
// painted opaque. The scrim is the one colour left translucent.
fn blend_over(fg: TRgba, bg: TRgba) -> TRgba {
    let a = fg.a;
    if a >= 1.0 {
        return fg;
    }
    TRgba {
        rgb: crate::theme::Rgb {
            r: fg.rgb.r * a + bg.rgb.r * (1.0 - a),
            g: fg.rgb.g * a + bg.rgb.g * (1.0 - a),
            b: fg.rgb.b * a + bg.rgb.b * (1.0 - a),
        },
        a: 1.0,
    }
}

// --- the render entry ---

pub fn render_app(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) -> impl IntoElement {
    let theme = view.theme.clone();
    let win_w = window.viewport_size().width.to_f64();
    let win_h = window.viewport_size().height.to_f64();
    let hero_focused = view.hero_focus.is_focused(window);

    let mut frame = Frame::default();
    let mut hits: Vec<AnyElement> = Vec::new();

    let caption_qt = qt_line(&view.font, theme.caption_size());
    let layout = main_layout(
        &theme,
        win_w,
        win_h,
        view.beats,
        caption_qt.height.ceil(),
        view.font.digit_ink(100.0).1,
        hero_ink_height(view),
        view.font.mono_advance(hero_font_size(view)),
        tap_implicit_width(view),
    );
    // The hero's line-box shift: -Math.round(tightBoundingRect.y - boundingRect.y).
    let hero_ascent = view.font.ascent(layout.hero_font_size);
    let (hero_ink_top, _) = view.font.digit_ink(layout.hero_font_size);
    let hero_y_shift = -(hero_ascent + hero_ink_top).round();
    let hero_baseline = layout.hero_box.y + hero_y_shift + hero_ascent;

    // --- the main view's visuals, then its hit areas ---
    paint_main_view(view, &theme, &layout, &mut frame, window, hero_focused, hero_baseline);
    hit_main_view(view, &layout, &mut hits, cx);

    // --- the dialogs, over everything while open ---
    if view.ts_open {
        paint_ground(&theme, &mut frame, win_w, win_h);
        let (py, ph) = paint_ts_dialog(view, &theme, &mut frame, window);
        hit_ground(&mut hits, cx, GroundKind::Ts);
        hit_ts_dialog(view, py, ph, &mut hits, cx);
    }
    if view.sub_open {
        paint_ground(&theme, &mut frame, win_w, win_h);
        let (py, ph) = paint_sub_dialog(view, &theme, &mut frame, window);
        hit_ground(&mut hits, cx, GroundKind::Sub);
        hit_sub_dialog(view, py, ph, &mut hits, cx);
    }
    if view.keys_open {
        paint_ground(&theme, &mut frame, win_w, win_h);
        let (py, ph) = paint_keys_dialog(view, &theme, &mut frame, window);
        hit_ground(&mut hits, cx, GroundKind::Keys);
        hit_keys_dialog(view, py, ph, &mut hits, cx);
    }

    // --- the keyboard focus frame, above all ---
    if view.tabbing && view.focus_index >= 0 {
        if let Some(rect) = focus_frame_rect(view, &layout) {
            frame.framed(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 },
                0.0,
                2.0,
                color(theme.color(ColorRole::Accent)),
            );
        }
    }

    // --- one canvas paints the whole frame ---
    let paint_frame = frame;
    let canvas_el = canvas(
        |_bounds, _window, _cx| (),
        move |_bounds, (), window: &mut Window, cx: &mut App| {
            for draw in &paint_frame.draws {
                match draw {
                    FDraw::Quad(q) => {
                        let (bw, bc) = q.border.unwrap_or((0.0, Rgba::default()));
                        window.paint_quad(quad(
                            Bounds {
                                origin: point(px(q.x as f32), px(q.y as f32)),
                                size: gpui::size(px(q.w as f32), px(q.h as f32)),
                            },
                            Corners::all(px(q.radius)),
                            q.color,
                            Edges::all(px(bw)),
                            bc,
                            gpui::BorderStyle::Solid,
                        ));
                    }
                    FDraw::Path(path, color) => {
                        window.paint_path(path.clone(), *color);
                    }
                }
            }
            for t in &paint_frame.texts {
                crate::text::paint_at(window, cx, &t.line, t.x, t.baseline);
            }
            for t in &paint_frame.wrapped {
                paint_wrapped(window, cx, &t.line, t.x, t.baseline, &QtLine { ascent: 0.0, descent: 0.0, height: t.line_h });
            }
        },
    );

    div()
        .id("winkel")
        .relative()
        .size_full()
        .bg(hsla(theme.color(ColorRole::Background)))
        .key_context("Main")
        .track_focus(&view.focus)
        .on_key_down(cx.listener(|view: &mut AppView, event: &gpui::KeyDownEvent, window, cx| {
            crate::app::handle_key(view, event, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|view: &mut AppView, _e: &gpui::MouseDownEvent, _w, cx| {
                // Taps anywhere keep the ring in step with the mouse.
                view.tabbing = false;
                view.focus_index = -1;
                cx.notify();
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|view: &mut AppView, _e: &gpui::MouseUpEvent, _w, cx| {
                let now = Instant::now();
                let num_was = view.num_wheel.is_dragging();
                let den_was = view.den_wheel.is_dragging();
                if num_was {
                    view.num_wheel.end_drag(now);
                    view.ts_draft_beats = view.num_wheel.picked();
                }
                if den_was {
                    view.den_wheel.end_drag(now);
                    view.ts_draft_denominator = view.den_wheel.picked();
                }
                if num_was || den_was {
                    drive_wheel_animations(view, cx);
                }
                view.pressed = None;
                cx.notify();
            }),
        )
        .on_mouse_move(cx.listener(|view: &mut AppView, e: &gpui::MouseMoveEvent, _w, cx| {
            // A wheel drag rides the window's mouse moves, the way a
            // ListView's drag continues outside its bounds.
            let mut changed = false;
            if view.num_wheel.is_dragging() && view.num_wheel.drag_to(e.position.y.to_f64()) {
                view.ts_draft_beats = view.num_wheel.picked();
                changed = true;
            }
            if view.den_wheel.is_dragging() && view.den_wheel.drag_to(e.position.y.to_f64()) {
                view.ts_draft_denominator = view.den_wheel.picked();
                changed = true;
            }
            if changed {
                cx.notify();
            }
        }))
        .child(canvas_el)
        .children(hits)
}

// --- measurement helpers ---

fn hero_font_size(view: &AppView) -> f64 {
    let probe = view.font.digit_ink(100.0).1.max(1.0);
    (view.theme.golden(4) * 100.0 / probe).round()
}

fn hero_ink_height(view: &AppView) -> f64 {
    view.font.digit_ink(hero_font_size(view)).1
}

fn tap_implicit_width(view: &AppView) -> f64 {
    let label = view.i18n.tr1("main.tap");
    let w = view.font.mono_advance(view.theme.body()) * label.chars().count() as f64;
    (w.ceil() + 2.0 * view.theme.gap() + 2.0 * view.theme.hairline()).max(view.theme.hit_min())
}

fn dialog_button_width(view: &AppView, label: &str, size: f64) -> f64 {
    // A monospace face's line width is its chars' advances; the QML's Text
    // implicit width is that value ceiled.
    let w = view.font.mono_advance(size) * label.chars().count() as f64;
    (w.ceil() + 2.0 * view.theme.gap() + 2.0 * view.theme.hairline()).max(view.theme.hit_min())
}

fn dialog_button_height(view: &AppView, size: f64) -> f64 {
    let text_h = qt_line(&view.font, size).height.ceil();
    (text_h + view.theme.gap() + 2.0 * view.theme.hairline()).max(view.theme.hit_min())
}

// --- the main view ---

fn paint_main_view(
    view: &mut AppView,
    theme: &Theme,
    l: &MainLayout,
    frame: &mut Frame,
    window: &mut Window,
    hero_focused: bool,
    hero_baseline: f64,
) {
    let bg = theme.color(ColorRole::Background);
    let accent = theme.color(ColorRole::Accent);
    let foreground = theme.color(ColorRole::Foreground);
    let line_soft = blend_over(theme.color(ColorRole::LineSoft), bg);

    // --- the meters: three bars from the bottom, the playhead on top ---
    let m = &l.meters;
    for i in 0..view.beats {
        let x = m.row.x + i as f64 * (m.rect_width + m.spacing);
        let voice = view.voices.get(i as usize).copied().unwrap_or(1);
        let is_now = view.running && view.current_beat == i as i32;
        for bar in 0..3 {
            let fill_order = 2 - bar as i32;
            let y = m.row.y + m.head_height + bar as f64 * (m.bar_height + m.bar_gap);
            let c = if fill_order < voice as i32 { accent } else { theme.color(ColorRole::LineSoft) };
            frame.quad(x, y, m.rect_width, m.bar_height, color(blend_over(c, bg)));
        }
        if is_now {
            frame.quad(x, m.row.y, m.rect_width, m.head_height, color(accent));
        }
    }

    // --- the tempo caption: the marking, in caps ---
    let band = tempo_band(view.bpm);
    let tempo_name = view.i18n.tr1(TEMPO_KEYS[band]).to_uppercase();
    let line = view.texts.line(window, &theme.font_family, &tempo_name, theme.caption_size(), false, hsla(theme.color(ColorRole::Caption)));
    frame.text_centered(line, l.content_center, l.caption.y + caption_ascent(view, theme));

    // --- the steppers ---
    for (rect, label) in [(&l.minus, "−"), (&l.plus, "+")] {
        frame.quad(rect.x, rect.y, rect.w, rect.h, color(line_soft));
        let qt = qt_line(&view.font, theme.golden(2));
        let line = view.texts.line(window, &theme.font_family, label, theme.golden(2), false, hsla(foreground));
        frame.text_centered(line, rect.x + rect.w / 2.0, rect.y + rect.h / 2.0 - qt.height / 2.0 + qt.ascent);
    }

    // --- the hero editor ---
    paint_hero(view, theme, &l.hero_box, hero_baseline, frame, window, hero_focused);

    // --- the transport: the accent circle and its mark ---
    let play = l.play;
    let scale = press_scale(view, Pressed::Play);
    let d = play.w * scale;
    frame.quad_radius(
        play.x + (play.w - d) / 2.0,
        play.y + (play.h - d) / 2.0,
        d,
        d,
        color(accent),
        (d / 2.0) as f32,
    );
    if let Some(path) = crate::play_mark::build_mark(view.running, (play.x + play.w / 2.0, play.y + play.h / 2.0, play.w)) {
        frame.path(path, hsla(bg));
    }

    // --- the meter and tap buttons ---
    let ts_label = SharedString::from(format!("{}/{}", view.beats, view.denominator)).to_string();
    paint_dialog_button(view, theme, frame, window, &l.ts_btn, &ts_label, theme.body(), true, view.ts_open, false, line_soft);
    let tap_label = view.i18n.tr1("main.tap");
    paint_dialog_button(view, theme, frame, window, &l.tap_btn, &tap_label, theme.body(), false, false, false, line_soft);

    // The subdivision button carries its figure, centred by its ink.
    frame.quad(l.sub_btn.x, l.sub_btn.y, l.sub_btn.w, l.sub_btn.h, color(line_soft));
    let fig = note_figure::geom(view.denominator, view.subdivision, view.subpattern, Some(view.subshape), theme.body(), &view.font, theme);
    let ink = if view.sub_open { accent } else { foreground };
    let ox = fig.ink.offset_x(fig.w);
    let oy = fig.ink.offset_y(fig.h);
    let x = l.sub_btn.x + l.sub_btn.w / 2.0 - fig.w / 2.0 - ox;
    let y = l.sub_btn.y + l.sub_btn.h / 2.0 - fig.h / 2.0 - oy;
    paint_figure(view, frame, &fig, x, y, hsla(ink), window);

    // --- the error line, when something is wrong ---
    let error = if !view.failure.is_empty() {
        view.failure.clone()
    } else if view.silent_output {
        view.i18n.tr1("error.silent")
    } else {
        String::new()
    };
    if !error.is_empty() {
        let wrapped = view.texts.wrapped(window, &theme.font_family, &error, theme.caption_size(), false, hsla(theme.color(ColorRole::Urgent)), l.content_w);
        let qt = qt_line(&view.font, theme.caption_size());
        frame.wrapped(wrapped, l.content_x, l.error_y + qt.ascent, qt.height);
    }
}

fn layout_ref(l: &MainLayout) -> &MainLayout {
    l
}

fn caption_ascent(view: &AppView, theme: &Theme) -> f64 {
    qt_line(&view.font, theme.caption_size()).ascent
}

// The hero editor: selection, digits, caret.
fn paint_hero(view: &mut AppView, theme: &Theme, box_rect: &Rect, baseline: f64, frame: &mut Frame, window: &mut Window, focused: bool) {
    let bg = theme.color(ColorRole::Background);
    let foreground = theme.color(ColorRole::Foreground);
    let text = view.hero.text.clone();
    let size = hero_font_size(view);
    let qt = qt_line(&view.font, size);

    let line = view.texts.line(window, &theme.font_family, &text, size, false, hsla(foreground));
    let text_w = line.width.to_f64();
    let text_left = box_rect.x + (box_rect.w - text_w) / 2.0;

    if focused {
        if let Some((a, b)) = view.hero.selection {
            if a != b {
                // QML's selection: accent at 0.35 over the background, the
                // selected text in the background's own ink.
                let sel = blend_over(theme.color(ColorRole::Accent).with_alpha(0.35), bg);
                frame.quad(
                    text_left + view.font.mono_advance(size) * a.min(b) as f64,
                    baseline - qt.ascent,
                    view.font.mono_advance(size) * (a.max(b) - a.min(b)) as f64,
                    qt.height,
                    color(sel),
                );
            }
        }
    }

    frame.text(line, text_left, baseline);

    if focused {
        let x = text_left + view.font.mono_advance(size) * view.hero.cursor as f64;
        frame.quad(x, baseline - qt.ascent, 1.0, qt.height, color(foreground));
    }
}

// One of the dialog-shaped buttons: a soft fill or a hairline frame, live
// text centred inside. Mirrors DialogButton.qml.
#[allow(clippy::too_many_arguments)]
fn paint_dialog_button(
    view: &mut AppView,
    theme: &Theme,
    frame: &mut Frame,
    window: &mut Window,
    rect: &Rect,
    label: &str,
    size: f64,
    demi: bool,
    primary: bool,
    framed: bool,
    fill: TRgba,
) {
    if framed {
        let border = if primary { theme.color(ColorRole::Accent) } else { theme.color(ColorRole::Muted) };
        frame.framed(rect.x, rect.y, rect.w, rect.h, Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }, 0.0, theme.hairline() as f32, color(border));
    } else {
        frame.quad(rect.x, rect.y, rect.w, rect.h, color(fill));
    }
    let ink = if primary { theme.color(ColorRole::Accent) } else { theme.color(ColorRole::Foreground) };
    let qt = qt_line(&view.font, size);
    let line = view.texts.line(window, &theme.font_family, label, size, demi, hsla(ink));
    frame.text_centered(line, rect.x + rect.w / 2.0, rect.y + rect.h / 2.0 - qt.height / 2.0 + qt.ascent);
}

// A figure through the frame: paths built now, painted with the frame.
fn paint_figure(view: &mut AppView, frame: &mut Frame, fig: &FigureGeom, x: f64, y: f64, ink: Hsla, window: &mut Window) {
    let (paths, texts) = note_figure::build(fig, point(px(x as f32), px(y as f32)), ink, &view.theme, window, &view.theme.font_family, &mut view.texts);
    for p in paths {
        frame.path(p, ink);
    }
    for (line, tx, baseline) in texts {
        frame.text(line, tx, baseline);
    }
}

fn press_scale(view: &AppView, which: Pressed) -> f64 {
    match view.pressed {
        Some((p, at)) if p == which => {
            let t = ((Instant::now() - at).as_secs_f64() / 0.15).min(1.0);
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            1.0 - 0.04 * eased
        }
        _ => 1.0,
    }
}

// --- hit areas: transparent divs with the handlers ---

fn hit_div(rect: &Rect, id: impl Into<ElementId>) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .absolute()
        .left(px(rect.x as f32))
        .top(px(rect.y as f32))
        .w(px(rect.w as f32))
        .h(px(rect.h as f32))
}

fn hit_main_view(view: &AppView, l: &MainLayout, out: &mut Vec<AnyElement>, cx: &mut Context<AppView>) {
    let m = &l.meters;
    for i in 0..view.beats as usize {
        let x = m.row.x + i as f64 * (m.rect_width + m.spacing);
        let rect = Rect { x, y: m.row.y, w: m.rect_width, h: m.row.h };
        out.push(
            hit_div(&rect, ElementId::Name(SharedString::from(format!("beat-{}", i))))
                .cursor(gpui::CursorStyle::PointingHand)
                .on_click(cx.listener(move |view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                    view.cycle_voice(i as u32, cx);
                }))
                .into_any_element(),
        );
    }
    out.push(
        hit_div(&l.minus, "bpm-minus")
            .cursor(gpui::CursorStyle::PointingHand)
            .on_click(cx.listener(|view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                view.set_bpm(view.bpm - 1.0, cx);
            }))
            .into_any_element(),
    );
    out.push(
        hit_div(&l.plus, "bpm-plus")
            .cursor(gpui::CursorStyle::PointingHand)
            .on_click(cx.listener(|view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                view.set_bpm(view.bpm + 1.0, cx);
            }))
            .into_any_element(),
    );
    out.push(
        hit_div(&l.play, "play")
            .cursor(gpui::CursorStyle::PointingHand)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view: &mut AppView, _e: &gpui::MouseDownEvent, _w, cx| {
                    view.pressed = Some((Pressed::Play, Instant::now()));
                    drive_press_animation(view, cx);
                }),
            )
            .on_click(cx.listener(|view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                view.send(&crate::protocol::Request::Toggle);
            }))
            .into_any_element(),
    );
    out.push(
        hit_div(&l.ts_btn, "ts")
            .cursor(gpui::CursorStyle::PointingHand)
            .on_click(cx.listener(|view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                view.toggle_ts(cx);
            }))
            .into_any_element(),
    );
    out.push(
        hit_div(&l.tap_btn, "tap")
            .cursor(gpui::CursorStyle::PointingHand)
            .on_click(cx.listener(|view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                view.tap(cx);
            }))
            .into_any_element(),
    );
    out.push(
        hit_div(&l.sub_btn, "sub")
            .cursor(gpui::CursorStyle::PointingHand)
            .on_click(cx.listener(|view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                view.toggle_sub(cx);
            }))
            .into_any_element(),
    );
    // The hero: an I-beam, a click gives the editor the focus.
    out.push(
        hit_div(&l.hero_box, "hero")
            .cursor(gpui::CursorStyle::IBeam)
            .on_click(cx.listener(|view: &mut AppView, _e: &gpui::ClickEvent, window, cx| {
                view.hero_focus.focus(window);
                cx.notify();
            }))
            .into_any_element(),
    );
}

// --- the dialogs ---

#[derive(Clone, Copy)]
enum GroundKind {
    Ts,
    Sub,
    Keys,
}

fn paint_ground(theme: &Theme, frame: &mut Frame, win_w: f64, win_h: f64) {
    // The one translucent colour in the frame: Flea's dimmed ground.
    let scrim = theme.color(ColorRole::Background).with_alpha(theme.ground_opacity());
    frame.quad(0.0, 0.0, win_w, win_h, color(scrim));
}

fn hit_ground(out: &mut Vec<AnyElement>, cx: &mut Context<AppView>, kind: GroundKind) {
    let rect = Rect { x: 0.0, y: 0.0, w: WINDOW_W, h: WINDOW_H };
    let id = match kind {
        GroundKind::Ts => "ground-ts",
        GroundKind::Sub => "ground-sub",
        GroundKind::Keys => "ground-keys",
    };
    out.push(
        hit_div(&rect, id)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view: &mut AppView, _e: &gpui::MouseDownEvent, _w, cx| {
                    dismiss(view, kind, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view: &mut AppView, _e: &gpui::MouseDownEvent, _w, cx| {
                    dismiss(view, kind, cx);
                }),
            )
            .into_any_element(),
    );
}

fn dismiss(view: &mut AppView, kind: GroundKind, cx: &mut Context<AppView>) {
    match kind {
        GroundKind::Ts => view.ts_open = false,
        GroundKind::Sub => view.sub_open = false,
        GroundKind::Keys => view.keys_open = false,
    }
    view.tabbing = false;
    view.focus_index = -1;
    cx.notify();
}

// One wheel, painted. The hairlines and the fading entries, exactly the
// QML's ListView with its highlight band.
fn paint_wheel(view: &mut AppView, theme: &Theme, frame: &mut Frame, window: &mut Window, which: Which, x: f64, y: f64) {
    let surface = theme.color(ColorRole::Surface);
    let line_soft = blend_over(theme.color(ColorRole::LineSoft), surface);
    let accent = theme.color(ColorRole::Accent);
    let foreground = theme.color(ColorRole::Foreground);

    frame.quad(x, y + BAND_TOP, WHEEL_WIDTH, 1.0, color(line_soft));
    frame.quad(x, y + BAND_TOP + CELL - 1.0, WHEEL_WIDTH, 1.0, color(line_soft));

    let wheel = match which {
        Which::Num => &view.num_wheel,
        Which::Den => &view.den_wheel,
    };
    let picked_ix = wheel.picked_ix();
    let offset = wheel.offset;
    let count = wheel.values.len() as i32;
    let first = (((offset / CELL).floor() as i32) - 1).clamp(0, count - 1);
    let last = (((offset + WHEEL_HEIGHT) / CELL).ceil() as i32 + 1).clamp(0, count - 1);
    for j in first..=last {
        let item_y = y + j as f64 * CELL - offset;
        let center_y = item_y + CELL / 2.0;
        let dist = (center_y - (y + WHEEL_HEIGHT / 2.0)).abs() / (WHEEL_HEIGHT / 2.0);
        let opacity = 0.3 + 0.7 * (1.0 - dist.min(1.0));
        let is_picked = j as usize == picked_ix;
        let sz = if is_picked { theme.body() } else { theme.body_small() };
        let c = blend_over(if is_picked { accent } else { foreground }.with_alpha(opacity), surface);
        let text = wheel.values[j as usize].to_string();
        let qt = qt_line(&view.font, sz);
        let line = view.texts.line(window, &theme.font_family, &text, sz, false, hsla(c));
        frame.text_centered(line, x + WHEEL_WIDTH / 2.0, center_y - qt.height / 2.0 + qt.ascent);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Which {
    Num,
    Den,
}

fn hit_wheel(view: &AppView, out: &mut Vec<AnyElement>, cx: &mut Context<AppView>, which: Which, x: f64, y: f64) {
    let _ = view;
    let rect = Rect { x, y, w: WHEEL_WIDTH, h: WHEEL_HEIGHT };
    let id = match which {
        Which::Num => "wheel-num",
        Which::Den => "wheel-den",
    };
    out.push(
        hit_div(&rect, id)
            .cursor(gpui::CursorStyle::PointingHand)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view: &mut AppView, e: &gpui::MouseDownEvent, _w, cx| {
                    match which {
                        Which::Num => view.num_wheel.begin_drag(e.position.y.to_f64()),
                        Which::Den => view.den_wheel.begin_drag(e.position.y.to_f64()),
                    }
                    cx.notify();
                }),
            )
            .on_scroll_wheel(
                cx.listener(move |view: &mut AppView, e: &gpui::ScrollWheelEvent, _w, cx| {
                    let raw = match e.delta {
                        gpui::ScrollDelta::Pixels(p) => -p.y.to_f64() / CELL,
                        gpui::ScrollDelta::Lines(l) => -l.y as f64,
                    };
                    let now = Instant::now();
                    match which {
                        Which::Num => {
                            view.num_wheel.scroll(raw, now);
                            view.ts_draft_beats = view.num_wheel.picked();
                        }
                        Which::Den => {
                            view.den_wheel.scroll(raw, now);
                            view.ts_draft_denominator = view.den_wheel.picked();
                        }
                    }
                    drive_wheel_animations(view, cx);
                }),
            )
            .on_click(cx.listener(move |view: &mut AppView, e: &gpui::ClickEvent, _w, cx| {
                // A click on a value spins it into the middle band.
                let (offset, top) = match which {
                    Which::Num => (view.num_wheel.offset, view.wheel_num_top),
                    Which::Den => (view.den_wheel.offset, view.wheel_den_top),
                };
                let ClickEvent::Mouse(m) = e else { return };
                let j = ((m.up.position.y.to_f64() - top + offset) / CELL).floor();
                let now = Instant::now();
                match which {
                    Which::Num => {
                        if j >= 0.0 && (j as usize) < view.num_wheel.values.len() {
                            view.num_wheel.select_ix(j as usize);
                            view.ts_draft_beats = view.num_wheel.picked();
                            drive_wheel_animations(view, cx);
                        }
                    }
                    Which::Den => {
                        if j >= 0.0 && (j as usize) < view.den_wheel.values.len() {
                            view.den_wheel.select_ix(j as usize);
                            view.ts_draft_denominator = view.den_wheel.picked();
                            drive_wheel_animations(view, cx);
                        }
                    }
                }
            }))
            .into_any_element(),
    );
}

// The time-signature card. Returns the panel's y and height.
fn paint_ts_dialog(view: &mut AppView, theme: &Theme, frame: &mut Frame, window: &mut Window) -> (f64, f64) {
    let surface = theme.color(ColorRole::Surface);
    let muted = theme.color(ColorRole::Muted);
    let caption_on_surface = theme.color(ColorRole::CaptionOnSurface);
    let foreground = theme.color(ColorRole::Foreground);

    let body_qt = qt_line(&view.font, theme.body());
    let title_h = body_qt.height.ceil();
    let header_h = title_h + theme.gap();
    let pad12 = theme.space(12.0);

    let cancel_w = dialog_button_width(view, &view.i18n.tr1("dialog.cancel"), theme.body());
    let ok_w = dialog_button_width(view, &view.i18n.tr1("dialog.ok"), theme.body());
    let btn_h = dialog_button_height(view, theme.body());

    let inner_h = pad12 + WHEEL_HEIGHT + pad12;
    let column_h = header_h + inner_h + btn_h;
    let panel_h = column_h + 2.0 * theme.row_padding_x();
    let panel_w = theme.space(296.0);
    let px0 = (WINDOW_W - panel_w) / 2.0;
    let py0 = (WINDOW_H - panel_h) / 2.0;

    frame.framed(px0, py0, panel_w, panel_h, color(surface), theme.corner_radius as f32, theme.hairline() as f32, color(muted));

    // The header: bold title on a hairline rule.
    let title = view.i18n.tr1("timeSignature.title");
    let line = view.texts.line(window, &theme.font_family, &title, theme.body(), true, hsla(foreground));
    frame.text(line, px0 + theme.row_padding_x(), py0 + theme.row_padding_x() + body_qt.ascent);
    frame.quad(px0, py0 + theme.row_padding_x() + header_h - theme.hairline(), panel_w, theme.hairline(), color(blend_over(muted.with_alpha(0.4), surface)));

    // The wheels, centred, with the slash between them.
    let slash_qt = qt_line(&view.font, theme.heading());
    let slash_line = view.texts.line(window, &theme.font_family, "/", theme.heading(), false, hsla(caption_on_surface));
    let slash_w = slash_line.width.to_f64();
    let row_w = WHEEL_WIDTH + theme.space(6.0) + slash_w + theme.space(6.0) + WHEEL_WIDTH;
    let row_x = px0 + (panel_w - row_w) / 2.0;
    let row_y = py0 + theme.row_padding_x() + header_h + pad12;

    view.wheel_num_top = row_y;
    view.wheel_den_top = row_y;
    let den_x = row_x + WHEEL_WIDTH + theme.space(6.0) + slash_w + theme.space(6.0);
    view.wheel_num_x = row_x;
    view.wheel_den_x = den_x;
    paint_wheel(view, theme, frame, window, Which::Num, row_x, row_y);
    paint_wheel(view, theme, frame, window, Which::Den, den_x, row_y);
    frame.text(slash_line, row_x + WHEEL_WIDTH + theme.space(6.0), row_y + WHEEL_HEIGHT / 2.0 - slash_qt.height / 2.0 + slash_qt.ascent);

    // The action row: Cancel then the primary, flush with the card's right.
    let actions_y = py0 + theme.row_padding_x() + header_h + inner_h;
    let ok_x = px0 + panel_w - theme.row_padding_x() - ok_w;
    let cancel_x = ok_x - theme.gap() - cancel_w;
    let surface_borrow = surface;
    paint_dialog_button(view, theme, frame, window, &Rect { x: cancel_x, y: actions_y, w: cancel_w, h: btn_h }, &view.i18n.tr1("dialog.cancel"), theme.body(), false, false, true, surface_borrow);
    paint_dialog_button(view, theme, frame, window, &Rect { x: ok_x, y: actions_y, w: ok_w, h: btn_h }, &view.i18n.tr1("dialog.ok"), theme.body(), false, true, true, surface_borrow);

    (py0, panel_h)
}

fn hit_ts_dialog(view: &mut AppView, panel_y: f64, panel_h: f64, out: &mut Vec<AnyElement>, cx: &mut Context<AppView>) {
    let num_x = view.wheel_num_x;
    let den_x = view.wheel_den_x;
    let row_y = view.wheel_num_top;
    hit_wheel(view, out, cx, Which::Num, num_x, row_y);
    hit_wheel(view, out, cx, Which::Den, den_x, row_y);

    let theme = view.theme.clone();
    let cancel_w = dialog_button_width(view, &view.i18n.tr1("dialog.cancel"), theme.body());
    let ok_w = dialog_button_width(view, &view.i18n.tr1("dialog.ok"), theme.body());
    let btn_h = dialog_button_height(view, theme.body());
    let panel_w = theme.space(296.0);
    let px0 = (WINDOW_W - panel_w) / 2.0;
    let actions_y = panel_y + panel_h - theme.row_padding_x() - btn_h;
    let ok_x = px0 + panel_w - theme.row_padding_x() - ok_w;
    let cancel_x = ok_x - theme.gap() - cancel_w;
    out.push(
        hit_div(&Rect { x: cancel_x, y: actions_y, w: cancel_w, h: btn_h }, "ts-cancel")
            .cursor(gpui::CursorStyle::PointingHand)
            .on_click(cx.listener(|view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                view.activate(crate::app::RingEntry::DialogCancel, cx);
            }))
            .into_any_element(),
    );
    out.push(
        hit_div(&Rect { x: ok_x, y: actions_y, w: ok_w, h: btn_h }, "ts-ok")
            .cursor(gpui::CursorStyle::PointingHand)
            .on_click(cx.listener(|view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                view.activate(crate::app::RingEntry::DialogOk, cx);
            }))
            .into_any_element(),
    );
}

// The subdivision card.
fn paint_sub_dialog(view: &mut AppView, theme: &Theme, frame: &mut Frame, window: &mut Window) -> (f64, f64) {
    let surface = theme.color(ColorRole::Surface);
    let muted = theme.color(ColorRole::Muted);
    let accent = theme.color(ColorRole::Accent);
    let foreground = theme.color(ColorRole::Foreground);
    let caption_on_surface = theme.color(ColorRole::CaptionOnSurface);
    let selected_fill = blend_over(theme.color(ColorRole::SelectedFill), surface);
    let hover_fill = blend_over(theme.color(ColorRole::HoverFill), surface);

    let body_qt = qt_line(&view.font, theme.body());
    let title_h = body_qt.height.ceil();
    let header_h = title_h + theme.gap();
    let gap = theme.gap();

    // The tiles: four to a row, in bands by grid.
    let tile_w = (theme.space(296.0) - 2.0 * theme.row_padding_x() - 3.0 * gap) / 4.0;
    let tile_h = theme.golden(4) - theme.golden(1);
    let tiles = view.sub_tiles();
    let per_row = 4usize;
    let rows_n = (tiles.len() as f64 / per_row as f64).ceil() as usize;
    let bands_h = rows_n as f64 * tile_h + (rows_n.max(1) - 1) as f64 * gap;

    // The notes row and the draft row, their heights the QML's.
    let small_qt = qt_line(&view.font, theme.body_small());
    let notes_h = theme.golden(2).max(small_qt.height.ceil());
    let tallest_fig = note_figure::geom(6, 6, 63, None, theme.body(), &view.font, theme);
    let draft_h = gap + theme.space(12.0) + tallest_fig.h.max(2.0 * small_qt.height);

    let cancel_w = dialog_button_width(view, &view.i18n.tr1("dialog.cancel"), theme.body());
    let ok_w = dialog_button_width(view, &view.i18n.tr1("dialog.ok"), theme.body());
    let btn_h = dialog_button_height(view, theme.body());

    let column_h = header_h + gap + bands_h + gap + gap + notes_h + theme.golden(-1) + draft_h + btn_h;
    let panel_h = column_h + 2.0 * theme.row_padding_x();
    let panel_w = theme.space(296.0);
    let px0 = (WINDOW_W - panel_w) / 2.0;
    let py0 = (WINDOW_H - panel_h) / 2.0;

    frame.framed(px0, py0, panel_w, panel_h, color(surface), theme.corner_radius as f32, theme.hairline() as f32, color(muted));

    // Header.
    let title = view.i18n.tr1("subdivision.title");
    let line = view.texts.line(window, &theme.font_family, &title, theme.body(), true, hsla(foreground));
    frame.text(line, px0 + theme.row_padding_x(), py0 + theme.row_padding_x() + body_qt.ascent);
    frame.quad(px0, py0 + theme.row_padding_x() + header_h - theme.hairline(), panel_w, theme.hairline(), color(blend_over(muted.with_alpha(0.4), surface)));

    // The tile bands.
    let band_x = px0 + theme.row_padding_x();
    let band_y = py0 + theme.row_padding_x() + header_h + gap;
    view.sub_tile_rects.clear();
    for (i, &(n, mask, shape)) in tiles.iter().enumerate() {
        let r = i / per_row;
        let c = i % per_row;
        let tx = band_x + c as f64 * (tile_w + gap);
        let ty = band_y + r as f64 * (tile_h + gap);
        view.sub_tile_rects.push((tx, ty, tile_w, tile_h));
        let chosen = view.sub_draft_division == n
            && view.sub_draft_mask == mask
            && rhythm::cell(view.sub_draft_division, view.sub_draft_mask, Some(view.sub_draft_shape)).shape == shape;
        let hovered = view.hovered_tile == Some(i as u32);
        let fill = if chosen {
            selected_fill
        } else if hovered {
            hover_fill
        } else {
            TRgba { rgb: surface.rgb, a: 0.0 }
        };
        frame.quad(tx, ty, tile_w, tile_h, color(fill));
        let fig = note_figure::geom(view.denominator, n, mask, Some(shape), theme.body(), &view.font, theme);
        let ink = if chosen { accent } else { foreground };
        // Tiles centre the box, not the ink.
        paint_figure(view, frame, &fig, tx + (tile_w - fig.w) / 2.0, ty + (tile_h - fig.h) / 2.0, hsla(ink), window);
    }

    // The rule under the bands.
    let rule_y = band_y + bands_h + gap - theme.hairline();
    frame.quad(px0, rule_y, panel_w, theme.hairline(), color(blend_over(muted.with_alpha(0.4), surface)));

    // The notes eyebrow and the tick squares.
    let notes_y = rule_y + theme.hairline() + gap;
    let eyebrow = view.i18n.tr1("subdivision.notes").to_uppercase();
    let tracking = theme.caption_size() * 0.14;
    let eyebrow_w = {
        let mut w = 0.0;
        for ch in eyebrow.chars() {
            w += view.font.mono_advance(theme.caption_size()) + tracking;
        }
        theme.golden(4).max(w + theme.golden(-1))
    };
    let eyebrow_baseline = notes_y + notes_h / 2.0 - small_qt.height / 2.0 + small_qt.ascent;
    paint_tracked_text(view, frame, window, &eyebrow, theme.caption_size(), tracking, px0 + theme.row_padding_x(), eyebrow_baseline, hsla(caption_on_surface));

    let mut sx = px0 + theme.row_padding_x() + eyebrow_w + theme.golden(-1);
    view.sub_slot_rects.clear();
    let starts = rhythm::starts(view.sub_draft_division, view.sub_draft_shape);
    for &slot in &starts {
        let on = (view.sub_draft_mask >> slot & 1) == 1;
        view.sub_slot_rects.push((sx, notes_y, theme.golden(2), theme.golden(2)));
        frame.framed(
            sx,
            notes_y,
            theme.golden(2),
            theme.golden(2),
            if on { color(accent) } else { Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 } },
            0.0,
            theme.hairline() as f32,
            color(if on { accent } else { muted }),
        );
        sx += theme.golden(2) + theme.golden(-1);
    }

    // The draft, drawn and named.
    let draft_y = notes_y + notes_h + theme.golden(-1);
    let draft_cell = rhythm::cell(view.sub_draft_division, view.sub_draft_mask, Some(view.sub_draft_shape));
    let draft_fig = note_figure::geom(view.denominator, view.sub_draft_division, view.sub_draft_mask, Some(draft_cell.shape), theme.body(), &view.font, theme);
    let row_offset = (theme.gap() - theme.space(12.0)) / 2.0;
    let row_center = draft_y + draft_h / 2.0 + row_offset;
    let name_w = panel_w - 2.0 * theme.row_padding_x() - theme.golden(4) - gap;
    let name = rhythm::cell_name(view.denominator, &draft_cell, &view.i18n);
    let name_wrapped = view.texts.wrapped(window, &theme.font_family, &name, theme.body_small(), false, hsla(foreground), name_w);
    let name_lines = name_wrapped.wrap_boundaries().len() + 1;
    let name_h = name_lines as f64 * small_qt.height;
    paint_figure(view, frame, &draft_fig, px0 + theme.row_padding_x(), row_center - draft_fig.h / 2.0, hsla(accent), window);
    frame.wrapped(
        name_wrapped,
        px0 + theme.row_padding_x() + draft_fig.w + gap,
        row_center - name_h / 2.0 + small_qt.ascent,
        small_qt.height,
    );

    // Actions.
    let actions_y = py0 + panel_h - theme.row_padding_x() - btn_h;
    let ok_x = px0 + panel_w - theme.row_padding_x() - ok_w;
    let cancel_x = ok_x - gap - cancel_w;
    paint_dialog_button(view, theme, frame, window, &Rect { x: cancel_x, y: actions_y, w: cancel_w, h: btn_h }, &view.i18n.tr1("dialog.cancel"), theme.body(), false, false, true, surface);
    paint_dialog_button(view, theme, frame, window, &Rect { x: ok_x, y: actions_y, w: ok_w, h: btn_h }, &view.i18n.tr1("dialog.ok"), theme.body(), false, true, true, surface);

    (py0, panel_h)
}

// Flea's eyebrow: bold and tracked. gpui shapes without letter spacing,
// so each glyph is placed by its own advance plus the tracking — the same
// total the QML's letterSpacing produces.
fn paint_tracked_text(
    view: &mut AppView,
    frame: &mut Frame,
    window: &mut Window,
    text: &str,
    size: f64,
    tracking: f64,
    x: f64,
    baseline: f64,
    ink: Hsla,
) {
    let mut cursor = x;
    for ch in text.chars() {
        let s = ch.to_string();
        let line = view.texts.line(window, &view.theme.font_family, &s, size, true, ink);
        frame.text(line, cursor, baseline);
        cursor += view.font.mono_advance(size) + tracking;
    }
}

fn hit_sub_dialog(view: &mut AppView, panel_y: f64, panel_h: f64, out: &mut Vec<AnyElement>, cx: &mut Context<AppView>) {
    let _ = (panel_y, panel_h);
    for (i, &(x, y, w, h)) in view.sub_tile_rects.clone().iter().enumerate() {
        out.push(
            hit_div(&Rect { x, y, w, h }, ElementId::Name(SharedString::from(format!("tile-{}", i))))
                .cursor(gpui::CursorStyle::PointingHand)
                .on_hover(cx.listener(move |view: &mut AppView, hovered: &bool, _w, cx| {
                    let next = if *hovered { Some(i as u32) } else { None };
                    if view.hovered_tile != next {
                        view.hovered_tile = next;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                    view.activate(crate::app::RingEntry::SubTile(i as u32), cx);
                }))
                .into_any_element(),
        );
    }
    for (i, &(x, y, w, h)) in view.sub_slot_rects.clone().iter().enumerate() {
        out.push(
            hit_div(&Rect { x, y, w, h }, ElementId::Name(SharedString::from(format!("slot-{}", i))))
                .cursor(gpui::CursorStyle::PointingHand)
                .on_click(cx.listener(move |view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                    view.activate(crate::app::RingEntry::SubSlot(i as u32), cx);
                }))
                .into_any_element(),
        );
    }
    let theme = view.theme.clone();
    let cancel_w = dialog_button_width(view, &view.i18n.tr1("dialog.cancel"), theme.body());
    let ok_w = dialog_button_width(view, &view.i18n.tr1("dialog.ok"), theme.body());
    let btn_h = dialog_button_height(view, theme.body());
    let panel_w = theme.space(296.0);
    let px0 = (WINDOW_W - panel_w) / 2.0;
    let actions_y = panel_y + panel_h - theme.row_padding_x() - btn_h;
    let ok_x = px0 + panel_w - theme.row_padding_x() - ok_w;
    let cancel_x = ok_x - theme.gap() - cancel_w;
    out.push(
        hit_div(&Rect { x: cancel_x, y: actions_y, w: cancel_w, h: btn_h }, "sub-cancel")
            .cursor(gpui::CursorStyle::PointingHand)
            .on_click(cx.listener(|view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                view.activate(crate::app::RingEntry::DialogCancel, cx);
            }))
            .into_any_element(),
    );
    out.push(
        hit_div(&Rect { x: ok_x, y: actions_y, w: ok_w, h: btn_h }, "sub-ok")
            .cursor(gpui::CursorStyle::PointingHand)
            .on_click(cx.listener(|view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                view.activate(crate::app::RingEntry::DialogOk, cx);
            }))
            .into_any_element(),
    );
}

// The keys sheet.
fn paint_keys_dialog(view: &mut AppView, theme: &Theme, frame: &mut Frame, window: &mut Window) -> (f64, f64) {
    let surface = theme.color(ColorRole::Surface);
    let muted = theme.color(ColorRole::Muted);
    let foreground = theme.color(ColorRole::Foreground);
    let caption_on_surface = theme.color(ColorRole::CaptionOnSurface);

    let body_qt = qt_line(&view.font, theme.body());
    let cap_h = (theme.body_small() * 1.45).round();
    let caption_qt = qt_line(&view.font, theme.caption_size());

    let bindings: &[(&str, &str)] = &[
        ("keycap.space", "keys.playStop"),
        ("keycap.upDown", "keys.tempo1"),
        ("keycap.shiftUpDown", "keys.tempo5"),
        ("keycap.pageUpDown", "keys.tempo10"),
        ("keycap.t", "keys.tapTempo"),
        ("keycap.beatUnits", "keys.beatUnit"),
        ("keycap.esc", "keys.stop"),
        ("keycap.tab", "keys.nextControl"),
        ("keycap.enter", "keys.press"),
        ("keycap.ctrlQ", "keys.quit"),
        ("keycap.question", "keys.sheet"),
    ];

    // The column is as wide as the widest cap in this language.
    let mut cap_w = cap_h;
    for (cap, _) in bindings {
        let text = view.i18n.shortcut(cap);
        let line = view.texts.line(window, &theme.font_family, &text, theme.caption_size(), false, Hsla::default());
        cap_w = cap_w.max(line.width.to_f64()).ceil() + theme.gap();
    }
    let action_wrap_w = theme.space(296.0) - 2.0 * theme.row_padding_x() - cap_w - theme.gap();

    let mut row_heights = Vec::new();
    for (_, action) in bindings {
        let text = view.i18n.tr1(action);
        let wrapped = view.texts.wrapped(window, &theme.font_family, &text, theme.caption_size(), false, hsla(caption_on_surface), action_wrap_w);
        let lines = wrapped.wrap_boundaries().len() + 1;
        row_heights.push(cap_h.max(lines as f64 * caption_qt.height));
    }
    let rows_spacing = theme.space(6.0);
    let rows_h: f64 = row_heights.iter().sum::<f64>() + (bindings.len() as f64 - 1.0) * rows_spacing;

    let header_h = body_qt.height.max(theme.hit_min()) + theme.gap();
    let column_h = header_h + theme.space(12.0) + rows_h;
    let panel_h = column_h + 2.0 * theme.row_padding_x();
    let panel_w = theme.space(296.0);
    let px0 = (WINDOW_W - panel_w) / 2.0;
    let py0 = (WINDOW_H - panel_h) / 2.0;

    frame.framed(px0, py0, panel_w, panel_h, color(surface), theme.corner_radius as f32, theme.hairline() as f32, color(muted));

    // Header: the title, then "esc" and the close mark at the right.
    let close_size = theme.hit_min();
    let close_y = py0 + theme.row_padding_x();
    let title = view.i18n.tr1("keys.title");
    let title_line = view.texts.line(window, &theme.font_family, &title, theme.body(), true, hsla(foreground));
    let title_baseline = close_y + close_size / 2.0 - body_qt.height / 2.0 + body_qt.ascent;
    frame.text(title_line, px0 + theme.row_padding_x(), title_baseline);

    let esc_line = view.texts.line(window, &theme.font_family, &view.i18n.tr1("keycap.esc"), theme.caption_size(), false, hsla(caption_on_surface));
    let esc_w = esc_line.width.to_f64();
    let close_x = px0 + panel_w - theme.gap() - close_size;
    let esc_x = close_x - theme.gap() - esc_w;
    frame.text(esc_line, esc_x, close_y + close_size / 2.0 - caption_qt.height / 2.0 + caption_qt.ascent);

    let close_line = view.texts.line(window, &theme.font_family, "✕", theme.body_small(), false, hsla(foreground));
    let close_w = close_line.width.to_f64();
    let close_qt = qt_line(&view.font, theme.body_small());
    frame.text(close_line, close_x + close_size / 2.0 - close_w / 2.0, close_y + close_size / 2.0 - close_qt.height / 2.0 + close_qt.ascent);

    frame.quad(px0, py0 + theme.row_padding_x() + header_h - theme.hairline(), panel_w, theme.hairline(), color(blend_over(muted.with_alpha(0.4), surface)));

    // The rows.
    let mut y = py0 + theme.row_padding_x() + header_h + theme.space(12.0);
    for ((cap, action), row_h) in bindings.iter().zip(row_heights.iter()) {
        frame.framed(
            px0 + theme.row_padding_x(),
            y,
            cap_w,
            *row_h,
            Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 },
            0.0,
            theme.hairline() as f32,
            color(muted),
        );
        let cap_text = view.i18n.shortcut(cap);
        let cap_line = view.texts.line(window, &theme.font_family, &cap_text, theme.caption_size(), false, hsla(foreground));
        let cw = cap_line.width.to_f64();
        frame.text(
            cap_line,
            px0 + theme.row_padding_x() + theme.hairline() + (cap_w - 2.0 * theme.hairline() - cw) / 2.0,
            y + theme.hairline() + (*row_h - 2.0 * theme.hairline()) / 2.0 - caption_qt.height / 2.0 + caption_qt.ascent,
        );

        let text = view.i18n.tr1(action);
        let wrapped = view.texts.wrapped(window, &theme.font_family, &text, theme.caption_size(), false, hsla(caption_on_surface), action_wrap_w);
        let top_pad = ((cap_h - caption_qt.height) / 2.0).max(0.0);
        frame.wrapped(
            wrapped,
            px0 + theme.row_padding_x() + cap_w + theme.gap(),
            y + top_pad + caption_qt.ascent,
            caption_qt.height,
        );
        y += *row_h + rows_spacing;
    }

    (py0, panel_h)
}

fn hit_keys_dialog(view: &mut AppView, panel_y: f64, _panel_h: f64, out: &mut Vec<AnyElement>, cx: &mut Context<AppView>) {
    let close_size = view.theme.hit_min();
    let panel_w = view.theme.space(296.0);
    let px0 = (WINDOW_W - panel_w) / 2.0;
    let close_x = px0 + panel_w - view.theme.gap() - close_size;
    let close_y = panel_y + view.theme.row_padding_x();
    out.push(
        hit_div(&Rect { x: close_x, y: close_y, w: close_size, h: close_size }, "keys-close")
            .cursor(gpui::CursorStyle::PointingHand)
            .on_click(cx.listener(|view: &mut AppView, _e: &gpui::ClickEvent, _w, cx| {
                view.keys_open = false;
                view.tabbing = false;
                view.focus_index = -1;
                cx.notify();
            }))
            .into_any_element(),
    );
}

// The focus frame's rect: the focused entry's own bounds, grown by 3.
fn focus_frame_rect(view: &AppView, l: &MainLayout) -> Option<Rect> {
    let entry = view.ring().get(view.focus_index.max(0) as usize).copied()?;
    let rect = match entry {
        crate::app::RingEntry::Beat(i) => {
            let m = &l.meters;
            Rect { x: m.row.x + i as f64 * (m.rect_width + m.spacing), y: m.row.y, w: m.rect_width, h: m.row.h }
        }
        crate::app::RingEntry::BpmMinus => l.minus,
        crate::app::RingEntry::BpmPlus => l.plus,
        crate::app::RingEntry::Play => l.play,
        crate::app::RingEntry::TimeSignature => l.ts_btn,
        crate::app::RingEntry::Tap => l.tap_btn,
        crate::app::RingEntry::Subdivision => l.sub_btn,
        _ => return None,
    };
    Some(Rect { x: rect.x - 3.0, y: rect.y - 3.0, w: rect.w + 6.0, h: rect.h + 6.0 })
}

// Keep frames coming while a wheel settles.
fn drive_wheel_animations(view: &mut AppView, cx: &mut Context<AppView>) {
    let task = cx.spawn(async move |this, cx| {
        for _ in 0..12 {
            cx.background_executor().timer(std::time::Duration::from_millis(16)).await;
            let more = this.update(cx, |view, cx| {
                let now = Instant::now();
                let a = view.num_wheel.tick(now);
                let b = view.den_wheel.tick(now);
                cx.notify();
                a || b
            });
            if !more.unwrap_or(false) {
                break;
            }
        }
    });
    view.tasks.push(task);
}

fn drive_press_animation(view: &mut AppView, cx: &mut Context<AppView>) {
    let task = cx.spawn(async move |this, cx| {
        for _ in 0..11 {
            cx.background_executor().timer(std::time::Duration::from_millis(16)).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        }
    });
    view.tasks.push(task);
}

const TEMPO_KEYS: [&str; 11] = [
    "tempo.larghissimo",
    "tempo.grave",
    "tempo.largo",
    "tempo.adagio",
    "tempo.andante",
    "tempo.moderato",
    "tempo.allegretto",
    "tempo.allegro",
    "tempo.vivace",
    "tempo.presto",
    "tempo.prestissimo",
];

pub fn tempo_band(bpm: f64) -> usize {
    const MAX: [f64; 11] = [20.0, 40.0, 60.0, 76.0, 108.0, 120.0, 156.0, 176.0, 200.0, 240.0, 9999.0];
    for (i, m) in MAX.iter().enumerate() {
        if bpm <= *m {
            return i;
        }
    }
    MAX.len() - 1
}
