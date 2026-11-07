// Text, the way the QML lays it out: shaped once per (string, size,
// weight, color), painted at a baseline the caller computes from the font
// metrics — QFontMetricsF's ascent is where QtQuick puts its baselines, so
// the same number decides every line here. All the words on screen come
// through I18n, as in the QML; this file only places them.

use std::collections::HashMap;

use gpui::{
    point, px, App, Font, FontFeatures, FontStyle, FontWeight, Hsla, Pixels, Point, ShapedLine,
    SharedString, TextAlign, TextRun, Window, WrappedLine,
};

use crate::font::FontMetrics;
use crate::theme::Theme;

#[derive(Clone, PartialEq, Eq, Hash)]
struct TextKey {
    text: SharedString,
    size_bits: u32,
    color_bits: u32,
    bold: bool,
    wrap_bits: u32,
}

#[derive(Clone)]
enum Cached {
    Line(ShapedLine),
    Wrapped(WrappedLine),
}

#[derive(Default)]
pub struct TextCache {
    cache: HashMap<TextKey, Cached>,
}

impl TextCache {
    pub fn invalidate(&mut self) {
        self.cache.clear();
    }

    fn run(family: &str, len: usize, bold: bool, color: Hsla) -> TextRun {
        TextRun {
            len,
            font: Font {
                family: family.to_string().into(),
                features: FontFeatures::default(),
                fallbacks: None,
                weight: if bold { FontWeight(700.0) } else { FontWeight(400.0) },
                style: FontStyle::default(),
            },
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    pub fn line(&mut self, window: &mut Window, family: &str, text: &str, size: f64, bold: bool, color: Hsla) -> ShapedLine {
        let key = TextKey {
            text: text.to_string().into(),
            size_bits: size.to_bits() as u32,
            color_bits: color_key(color),
            bold,
            wrap_bits: 0,
        };
        if let Some(Cached::Line(line)) = self.cache.get(&key) {
            return line.clone();
        }
        let run = Self::run(family, text.len(), bold, color);
        let line = window.text_system().shape_line(text.to_string().into(), px(size as f32), &[run], None);
        self.cache.insert(key, Cached::Line(line.clone()));
        line
    }

    // A wrapped paragraph at a given width, for the error line and the
    // keys sheet's descriptions.
    pub fn wrapped(&mut self, window: &mut Window, family: &str, text: &str, size: f64, bold: bool, color: Hsla, wrap_width: f64) -> WrappedLine {
        let key = TextKey {
            text: text.to_string().into(),
            size_bits: size.to_bits() as u32,
            color_bits: color_key(color),
            bold,
            wrap_bits: wrap_width.to_bits() as u32,
        };
        if let Some(Cached::Wrapped(w)) = self.cache.get(&key) {
            return w.clone();
        }
        let run = Self::run(family, text.len(), bold, color);
        let wrapped = window
            .text_system()
            .shape_text(text.to_string().into(), px(size as f32), &[run], Some(px(wrap_width as f32)), None)
            .ok()
            .and_then(|lines| lines.first().cloned())
            .unwrap_or_default();
        self.cache.insert(key, Cached::Wrapped(wrapped.clone()));
        wrapped
    }
}

fn color_key(c: Hsla) -> u32 {
    c.h.to_bits() ^ c.s.to_bits().rotate_left(8) ^ c.l.to_bits().rotate_left(16) ^ c.a.to_bits().rotate_left(24)
}

// The Qt line box: ascent, descent, height — the numbers QML Text uses to
// place baselines. cosmic-text rounds its own, so the QML's fractional
// metrics come from the font tables via FontMetrics instead.
pub struct QtLine {
    pub ascent: f64,
    pub descent: f64,
    pub height: f64,
}

pub fn qt_line(font: &FontMetrics, size: f64) -> QtLine {
    QtLine {
        ascent: font.ascent(size),
        descent: font.descent(size),
        height: font.line_height(size),
    }
}

// Paint a shaped line so its baseline sits at `baseline_y`, centred
// horizontally on `center_x` — the anchors.horizontalCenter case.
pub fn paint_centered(window: &mut Window, cx: &mut App, line: &ShapedLine, center_x: f64, baseline_y: f64) {
    let w = line.width.to_f64();
    paint_at(window, cx, line, center_x - w / 2.0, baseline_y);
}

// Paint with the line's left edge at x, baseline at baseline_y.
pub fn paint_at(window: &mut Window, cx: &mut App, line: &ShapedLine, x: f64, baseline_y: f64) {
    // ShapedLine::paint centres the glyphs in the given line height; pass
    // the shaped line's own ascent+descent so nothing is padded, and put
    // the origin where the baseline lands.
    let shaped_ascent = line.ascent.to_f64();
    let shaped_descent = line.descent.to_f64();
    let line_height = px((shaped_ascent + shaped_descent) as f32);
    let origin = point(px(x as f32), px((baseline_y - shaped_ascent) as f32));
    let _ = line.paint(origin, line_height, window, cx);
}

// Paint a wrapped paragraph: left edge at x, first baseline at baseline_y,
// line boxes qt height apart — QML Text stacks its lines at font height.
pub fn paint_wrapped(window: &mut Window, cx: &mut App, wrapped: &WrappedLine, x: f64, baseline_y: f64, qt: &QtLine) {
    let line_h = px(qt.height as f32);
    let shaped_ascent = wrapped.unwrapped_layout.ascent.to_f64();
    let origin = point(px(x as f32), px((baseline_y - shaped_ascent) as f32));
    let _ = wrapped.paint(origin, line_h, TextAlign::Left, None, window, cx);
}

pub fn point_px(x: f64, y: f64) -> Point<Pixels> {
    point(px(x as f32), px(y as f32))
}
