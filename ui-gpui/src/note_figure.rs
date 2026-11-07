// A rhythm figure, drawn, not typed — the port of NoteFigure.qml. The cell
// one beat is, with heads, stems, beams or flags, dots, rests, and a
// triplet's 3. The drawing compiles once into a list of primitives; the
// painter turns them into gpui paths and the measurer reads the same list
// for the ink box, which is how holders centre the notes rather than the
// box — the same trick the QML plays when its canvas measures its own
// pixels after every paint.

use gpui::{point, px, PathBuilder, PathStyle, StrokeOptions};

use crate::font::FontMetrics;
use crate::rhythm::{self, Cell};
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct V2 {
    pub x: f64,
    pub y: f64,
}

fn v(x: f64, y: f64) -> V2 {
    V2 { x, y }
}

#[derive(Clone, Debug)]
pub enum Prim {
    // A stroke with round caps (and round joins when it bends), the
    // canvas' default. Polyline points in figure coordinates.
    StrokeRound(Vec<V2>, f64),
    // A butt-cap segment: the beams' own cap, drawn half a thin longer
    // each way, exactly as the QML's moveTo/lineTo does it.
    StrokeButt(Vec<V2>, f64),
    // A quadratic stroke with round caps: flags and rest hooks.
    StrokeQuad(V2, V2, V2, f64),
    FillEllipse(V2, f64, f64, f64), // centre, rx, ry, rotation (radians)
    StrokeEllipse(V2, f64, f64, f64, f64),
    FillCircle(V2, f64),
    // The tuplet's number: text, centre x, baseline y, pixel size, bold.
    Text(String, V2, f64),
}

#[derive(Clone, Debug)]
pub struct FigureGeom {
    pub cell: Cell,
    pub beat_value: u32,
    pub unit: f64,
    pub w: f64,
    pub h: f64,
    pub tuplet: u32,
    pub prims: Vec<Prim>,
    // The ink box, in the box's own coordinates, pixel bounds the way the
    // QML's getImageData scan reports them.
    pub ink: InkBox,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InkBox {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

impl InkBox {
    pub fn width(&self) -> i32 {
        self.max_x - self.min_x + 1
    }
    pub fn height(&self) -> i32 {
        self.max_y - self.min_y + 1
    }
    // The offset a holder applies so the ink's centre sits on the box's
    // centre: Math.round(inkX + inkWidth/2 - width/2).
    pub fn offset_x(&self, w: f64) -> f64 {
        (self.min_x as f64 + self.width() as f64 / 2.0 - w / 2.0).round()
    }
    pub fn offset_y(&self, h: f64) -> f64 {
        (self.min_y as f64 + self.height() as f64 / 2.0 - h / 2.0).round()
    }
}

pub fn value_of(beat_value: u32, it: &rhythm::FigureItem) -> f64 {
    beat_value as f64 * it.k
}

// Quaver one beam, semiquaver two, demisemiquaver three.
pub fn beams_of(beat_value: u32, it: &rhythm::FigureItem) -> u32 {
    let v = value_of(beat_value, it);
    if v >= 8.0 {
        (v / 4.0).log2().round() as u32
    } else {
        0
    }
}

fn stem_x(cx: f64, head_rx: f64) -> f64 {
    cx + head_rx * 0.85
}

// The head's centre: the canvas translate(cx, baseY) puts it there, and
// Qt's Context2D.ellipse(x, y, w, h) is bounding-rectangle semantics, so
// the rotation about the local origin moves nothing. The head sits on the
// beat's baseline, tilted, with the stem meeting its right shoulder.
fn head_center(cx: f64, base_y: f64, _head_rx: f64, _head_ry: f64) -> V2 {
    v(cx, base_y)
}

// The figure: geometry, primitives, ink. Everything the painter and the
// measurer agree on.
pub fn geom(beat_value: u32, division: u32, mask: u32, shape: Option<u32>, unit: f64, font: &FontMetrics, theme: &Theme) -> FigureGeom {
    let cell = rhythm::cell(division, mask, shape);
    let items = &cell.items;
    let tuplet = rhythm::tuplet(division);

    let head_rx = unit * 0.32;
    let head_ry = unit * 0.23;
    let stem_h = unit * 1.15;
    // Five or six notes close ranks, so a sextuplet still fits its button.
    let step = unit * if items.len() > 4 { 0.68 } else { 0.95 };
    let beam_gap = unit * 0.28;
    let triplet_room = if tuplet > 0 { unit * 0.7 } else { 0.0 };

    let dots = items.iter().filter(|it| it.dot).count() as f64;
    let w = (head_rx * 2.0 + (items.len().max(1) as f64 - 1.0) * step + unit * 0.5 + dots * unit * 0.2).ceil();
    let h = (head_ry * 2.0 + stem_h + triplet_room + unit * 0.2).ceil();

    let base_y = h - head_ry - unit * 0.1;
    let stem_top = base_y - stem_h;
    let thin = (unit / 12.0).max(1.0);

    // Positions: one step per item, a little more after a dot.
    let mut xs = Vec::with_capacity(items.len());
    let mut x = head_rx + unit * 0.1;
    for it in items {
        xs.push(x);
        x += step + if it.dot { unit * 0.2 } else { 0.0 };
    }

    let mut prims: Vec<Prim> = Vec::new();

    for (i, it) in items.iter().enumerate() {
        let cx = xs[i];
        let val = value_of(beat_value, it);
        if it.rest {
            draw_rest(&mut prims, cx, base_y, unit, thin, val);
            continue;
        }
        let hc = head_center(cx, base_y, head_rx, head_ry);
        if val <= 2.0 {
            prims.push(Prim::StrokeEllipse(hc, head_rx, head_ry, -0.35, thin * 1.4));
        } else {
            prims.push(Prim::FillEllipse(hc, head_rx, head_ry, -0.35));
        }
        if val >= 2.0 {
            let sx = stem_x(cx, head_rx);
            prims.push(Prim::StrokeRound(vec![v(sx, base_y - head_ry * 0.4), v(sx, stem_top)], thin));
        }
        if it.dot {
            prims.push(Prim::FillCircle(v(cx + head_rx + unit * 0.22, base_y - unit * 0.12), thin * 1.1));
        }
    }

    // Beams join the notes of the cell, across a rest between them; a
    // level a note has and its neighbours lack is a stub toward the note
    // it belongs with; a note beamed to nothing carries flags.
    let notes: Vec<usize> = items.iter().enumerate().filter(|(_, it)| !it.rest).map(|(i, _)| i).collect();
    let mut max_beams = 0;
    for &i in &notes {
        max_beams = max_beams.max(beams_of(beat_value, &items[i]));
    }
    for level in 1..=max_beams {
        let y = stem_top + (level as f64 - 1.0) * beam_gap + thin;
        for (a, &ia) in notes.iter().enumerate() {
            if beams_of(beat_value, &items[ia]) < level {
                continue;
            }
            let prev = a > 0 && beams_of(beat_value, &items[notes[a - 1]]) >= level;
            let next = a + 1 < notes.len() && beams_of(beat_value, &items[notes[a + 1]]) >= level;
            if next {
                let x1 = stem_x(xs[ia], head_rx);
                let x2 = stem_x(xs[notes[a + 1]], head_rx);
                prims.push(Prim::StrokeButt(vec![v(x1 - thin / 2.0, y), v(x2 + thin / 2.0, y)], thin * 2.2));
            } else if !prev {
                if notes.len() == 1 {
                    let x = stem_x(xs[ia], head_rx);
                    prims.push(Prim::StrokeQuad(
                        v(x, y),
                        v(x + unit * 0.35, y + unit * 0.15),
                        v(x + unit * 0.3, y + unit * 0.55),
                        thin * 1.6,
                    ));
                } else {
                    // A stub: left when a note stands before it, else right.
                    let x = stem_x(xs[ia], head_rx);
                    let dir = if a > 0 { -1.0 } else { 1.0 };
                    prims.push(Prim::StrokeButt(vec![v(x, y), v(x + dir * unit * 0.45, y)], thin * 2.2));
                }
            }
        }
    }

    // The tuplet's number over the cell, with a bracket when the cell is
    // not one beamed group: a rest or an unbeamed note in it.
    if tuplet > 0 {
        let left = xs[0] - head_rx * 0.2;
        let right = xs[items.len() - 1] + head_rx;
        let mid = (left + right) / 2.0;
        let ty = stem_top - unit * 0.25;
        let size = (unit * 0.6).max(theme.caption_size() * 0.75).round();
        prims.push(Prim::Text(tuplet.to_string(), v(mid, ty), size));

        let mut beamed = notes.len() == items.len() && notes.len() > 1 && max_beams > 0;
        for &q in &notes {
            if beamed && beams_of(beat_value, &items[q]) == 0 {
                beamed = false;
            }
        }
        if !beamed {
            prims.push(Prim::StrokeRound(
                vec![v(left, ty + unit * 0.1), v(left, ty - unit * 0.2), v(mid - unit * 0.35, ty - unit * 0.2)],
                thin,
            ));
            prims.push(Prim::StrokeRound(
                vec![v(mid + unit * 0.35, ty - unit * 0.2), v(right, ty - unit * 0.2), v(right, ty + unit * 0.1)],
                thin,
            ));
        }
    }

    let ink = measure(&prims, font);
    FigureGeom { cell, beat_value, unit, w, h, tuplet, prims, ink }
}

// Rests: a crotchet's zigzag, a quaver's hooked stroke, one hook more for
// each halving after that.
fn draw_rest(prims: &mut Vec<Prim>, cx: f64, base_y: f64, unit: f64, thin: f64, val: f64) {
    let top = base_y - unit * 1.0;
    if val <= 4.0 {
        prims.push(Prim::StrokeRound(
            vec![
                v(cx - unit * 0.12, top + unit * 0.05),
                v(cx + unit * 0.18, top + unit * 0.35),
                v(cx - unit * 0.1, top + unit * 0.6),
                v(cx + unit * 0.14, top + unit * 0.9),
            ],
            thin * 2.0,
        ));
        return;
    }
    let hooks = (val / 4.0).log2().round() as i32;
    prims.push(Prim::StrokeRound(
        vec![v(cx + unit * 0.28, top + unit * 0.15), v(cx - unit * 0.05, top + unit * 1.0)],
        thin * 1.3,
    ));
    for h in 0..hooks {
        let hy = top + unit * 0.15 + h as f64 * unit * 0.3;
        let hx = cx + unit * 0.28 - h as f64 * unit * 0.1;
        prims.push(Prim::FillCircle(v(hx - unit * 0.22, hy), thin * 1.4));
        prims.push(Prim::StrokeQuad(
            v(hx - unit * 0.22, hy),
            v(hx - unit * 0.05, hy + unit * 0.14),
            v(hx, hy),
            thin,
        ));
    }
}

// A quadratic Bézier's exact bounding box, so flags and rest hooks measure
// to their true ink rather than their control polygon.
fn quad_bounds(p0: V2, p1: V2, p2: V2) -> (f64, f64, f64, f64) {
    let mut min_x = p0.x.min(p2.x);
    let mut max_x = p0.x.max(p2.x);
    let mut min_y = p0.y.min(p2.y);
    let mut max_y = p0.y.max(p2.y);
    for (a0, a1, a2) in [(p0.x, p1.x, p2.x), (p0.y, p1.y, p2.y)] {
        let denom = a0 - 2.0 * a1 + a2;
        if denom.abs() > 1e-9 {
            let t = ((a0 - a1) / denom).clamp(0.0, 1.0);
            let val = (1.0 - t) * (1.0 - t) * a0 + 2.0 * t * (1.0 - t) * a1 + t * t * a2;
            min_x = min_x.min(val);
            max_x = max_x.max(val);
            min_y = min_y.min(val);
            max_y = max_y.max(val);
        }
    }
    (min_x, min_y, max_x, max_y)
}

// The ink box, computed the way the QML's getImageData scan reports it: a
// pixel counts when its alpha passes 24/255, which puts the measured edge
// a little beyond the geometry on an antialiased stroke. EDGE is the
// coverage of that threshold on a straight edge.
fn measure(prims: &[Prim], font: &FontMetrics) -> InkBox {
    const EDGE: f64 = 0.41;
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut put = |x0: f64, y0: f64, x1: f64, y1: f64| {
        if x1 - x0 < 0.0 || y1 - y0 < 0.0 {
            return;
        }
        min_x = min_x.min(x0);
        min_y = min_y.min(y0);
        max_x = max_x.max(x1);
        max_y = max_y.max(y1);
    };

    for prim in prims {
        match prim {
            Prim::StrokeRound(pts, width) | Prim::StrokeButt(pts, width) => {
                let half = *width / 2.0;
                for p in pts {
                    put(p.x - half - EDGE, p.y - half - EDGE, p.x + half + EDGE, p.y + half + EDGE);
                }
            }
            Prim::StrokeQuad(p0, p1, p2, width) => {
                let (qx0, qy0, qx1, qy1) = quad_bounds(*p0, *p1, *p2);
                let half = *width / 2.0;
                put(qx0 - half - EDGE, qy0 - half - EDGE, qx1 + half + EDGE, qy1 + half + EDGE);
            }
            Prim::FillEllipse(c, rx, ry, rot) => {
                let (hx, hy) = rotated_half_extents(*rx, *ry, *rot);
                put(c.x - hx - EDGE, c.y - hy - EDGE, c.x + hx + EDGE, c.y + hy + EDGE);
            }
            Prim::StrokeEllipse(c, rx, ry, rot, width) => {
                let (hx, hy) = rotated_half_extents(*rx, *ry, *rot);
                let half = *width / 2.0;
                put(c.x - hx - half - EDGE, c.y - hy - half - EDGE, c.x + hx + half + EDGE, c.y + hy + half + EDGE);
            }
            Prim::FillCircle(c, r) => {
                put(c.x - *r - EDGE, c.y - *r - EDGE, c.x + *r + EDGE, c.y + *r + EDGE);
            }
            Prim::Text(text, at, size) => {
                let adv = font.mono_advance(*size) * text.chars().count() as f64;
                let (ink_top, ink_h) = font.digit_ink(*size);
                let half = adv / 2.0;
                put(at.x - half - EDGE, at.y + ink_top - EDGE, at.x + half + EDGE, at.y + ink_top + ink_h + EDGE);
            }
        }
    }

    if !min_x.is_finite() {
        return InkBox { min_x: 0, min_y: 0, max_x: 0, max_y: 0 };
    }
    InkBox {
        min_x: (min_x - EDGE).ceil() as i32,
        min_y: (min_y - EDGE).ceil() as i32,
        max_x: (max_x + EDGE).floor() as i32,
        max_y: (max_y + EDGE).floor() as i32,
    }
}

fn rotated_half_extents(rx: f64, ry: f64, rot: f64) -> (f64, f64) {
    let (c, s) = (rot.cos(), rot.sin());
    (
        ((rx * c).powi(2) + (ry * s).powi(2)).sqrt(),
        ((rx * s).powi(2) + (ry * c).powi(2)).sqrt(),
    )
}

// Build the figure's drawables at `origin` (the box's top-left): the paths
// for the painter, and the tuplet's number as a shaped line with its
// position. Runs during render, so the frame's paint pass stays dumb.
pub fn build(
    g: &FigureGeom,
    origin: gpui::Point<gpui::Pixels>,
    ink: gpui::Hsla,
    theme: &Theme,
    window: &mut gpui::Window,
    font_family: &str,
    texts: &mut crate::text::TextCache,
) -> (Vec<gpui::Path<gpui::Pixels>>, Vec<(gpui::ShapedLine, f64, f64)>) {
    use gpui::{point, px, PathBuilder, PathStyle, StrokeOptions};

    let mut paths = Vec::new();
    let mut shaped = Vec::new();
    let ox = origin.x.to_f64();
    let oy = origin.y.to_f64();
    let to_px = |p: V2| point(px((p.x + ox) as f32), px((p.y + oy) as f32));

    for prim in &g.prims {
        match prim {
            Prim::StrokeRound(pts, width) => {
                let opts = StrokeOptions::default()
                    .with_line_width(*width as f32)
                    .with_line_cap(lyon_path::LineCap::Round)
                    .with_line_join(lyon_path::LineJoin::Round);
                if let Some(path) = stroke_path(&to_px, pts, opts, false) {
                    paths.push(path);
                }
            }
            Prim::StrokeButt(pts, width) => {
                let opts = StrokeOptions::default().with_line_width(*width as f32);
                if let Some(path) = stroke_path(&to_px, pts, opts, false) {
                    paths.push(path);
                }
            }
            Prim::StrokeQuad(p0, p1, p2, width) => {
                let opts = StrokeOptions::default()
                    .with_line_width(*width as f32)
                    .with_line_cap(lyon_path::LineCap::Round);
                let mut b = PathBuilder::default().with_style(PathStyle::Stroke(opts));
                b.move_to(to_px(*p0));
                b.curve_to(to_px(*p2), to_px(*p1));
                if let Ok(path) = b.build() {
                    paths.push(path);
                }
            }
            Prim::FillEllipse(c, rx, ry, rot) => {
                let mut b = PathBuilder::fill();
                ellipse_subpath(&mut b, &to_px, *c, *rx, *ry, *rot);
                b.close();
                if let Ok(path) = b.build() {
                    paths.push(path);
                }
            }
            Prim::StrokeEllipse(c, rx, ry, rot, width) => {
                let opts = StrokeOptions::default().with_line_width(*width as f32);
                let mut b = PathBuilder::default().with_style(PathStyle::Stroke(opts));
                ellipse_subpath(&mut b, &to_px, *c, *rx, *ry, *rot);
                b.close();
                if let Ok(path) = b.build() {
                    paths.push(path);
                }
            }
            Prim::FillCircle(c, r) => {
                let mut b = PathBuilder::fill();
                ellipse_subpath(&mut b, &to_px, *c, *r, *r, 0.0);
                b.close();
                if let Ok(path) = b.build() {
                    paths.push(path);
                }
            }
            Prim::Text(text, at, size) => {
                let line = texts.line(window, font_family, text, *size, true, ink);
                let adv = line.width.to_f64();
                shaped.push((line, at.x + ox - adv / 2.0, at.y + oy));
            }
        }
    }
    (paths, shaped)
}

fn stroke_path(
    to_px: &impl Fn(V2) -> gpui::Point<gpui::Pixels>,
    pts: &[V2],
    opts: gpui::StrokeOptions,
    closed: bool,
) -> Option<gpui::Path<gpui::Pixels>> {
    if pts.len() < 2 {
        return None;
    }
    let mut b = PathBuilder::default().with_style(PathStyle::Stroke(opts));
    b.move_to(to_px(pts[0]));
    for p in &pts[1..] {
        b.line_to(to_px(*p));
    }
    if closed {
        b.close();
    }
    b.build().ok()
}

fn ellipse_subpath(
    b: &mut gpui::PathBuilder,
    to_px: &impl Fn(V2) -> gpui::Point<gpui::Pixels>,
    c: V2,
    rx: f64,
    ry: f64,
    rot: f64,
) {
    let deg = px((rot * 180.0 / std::f64::consts::PI) as f32);
    let radii = gpui::point(px(rx as f32), px(ry as f32));
    let right = v(c.x + rx, c.y);
    let left = v(c.x - rx, c.y);
    b.move_to(to_px(right));
    b.arc_to(radii, deg, false, true, to_px(left));
    b.arc_to(radii, deg, false, true, to_px(right));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn font() -> FontMetrics {
        FontMetrics::resolve("JetBrainsMono Nerd Font").expect("the theme font should resolve")
    }

    #[test]
    fn a_quarter_beats_figure_is_one_note() {
        let theme = Theme::new(std::path::PathBuf::from("/nonexistent"));
        let g = geom(4, 1, 0b1, None, 12.0, &font(), &theme);
        assert_eq!(g.cell.items.len(), 1);
        assert!(!g.cell.items[0].rest);
        // One filled head, one stem, nothing else.
        let fills = g.prims.iter().filter(|p| matches!(p, Prim::FillEllipse(..))).count();
        let stems = g.prims.iter().filter(|p| matches!(p, Prim::StrokeRound(pts, w) if pts.len() == 2)).count();
        assert_eq!(fills, 1);
        assert_eq!(stems, 1);
        // The ink is inside the box and the box is at least the ink.
        assert!(g.ink.min_x >= 0 && g.ink.max_x < g.w.ceil() as i32 + 1);
        assert!(g.ink.height() > 8);
    }

    #[test]
    fn a_triplet_names_three_and_draws_a_three() {
        let theme = Theme::new(std::path::PathBuf::from("/nonexistent"));
        let g = geom(4, 3, 0b111, None, 12.0, &font(), &theme);
        assert_eq!(g.tuplet, 3);
        assert!(g.prims.iter().any(|p| matches!(p, Prim::Text(t, ..) if t == "3")));
        // Fully beamed: a bracket is not drawn.
        assert_eq!(g.prims.iter().filter(|p| matches!(p, Prim::StrokeButt(..))).count(), 2);
    }

    #[test]
    fn rests_draw_their_hooks() {
        let theme = Theme::new(std::path::PathBuf::from("/nonexistent"));
        // mask 0b1001, shape 0b1111: the middle two slots are rests.
        let g = geom(4, 4, 0b1001, Some(0b1111), 12.0, &font(), &theme);
        assert_eq!(g.cell.items.iter().filter(|it| it.rest).count(), 2);
        assert!(g.prims.iter().any(|p| matches!(p, Prim::FillCircle(..))));
    }

    #[test]
    fn the_ink_centres_where_the_qml_would_centre() {
        let theme = Theme::new(std::path::PathBuf::from("/nonexistent"));
        let g = geom(4, 6, 0b111111, None, 12.0, &font(), &theme);
        let dx = g.ink.offset_x(g.w);
        // A sextuplet's flags live on the right, so the ink's centre sits
        // left of the box's; the offset moves it back to true.
        assert!(dx.abs() < g.w / 2.0);
    }
}
