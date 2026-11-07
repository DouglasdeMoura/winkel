// The transport's mark: Phosphor's "play" and "stop" icons, fill weight,
// the same paths PlayMark.qml carries in its PathSvg strings. Drawn as
// filled paths rather than typed glyphs, for the same reasons the QML
// gives: a font's marks are not centred on their own ink and may be
// missing altogether.

use crate::note_figure::V2;

fn v(x: f64, y: f64) -> V2 {
    V2 { x, y }
}

// The play triangle's height, base to tip, as a fraction of the circle.
pub const MARK: f64 = 0.24;
// Phosphor's ink inside its 256 grid: the triangle is 208.1 tall.
const TRIANGLE_HEIGHT: f64 = 208.1;

pub fn mark_box(width: f64) -> f64 {
    width * MARK * 256.0 / TRIANGLE_HEIGHT
}

#[derive(Clone, Copy)]
enum Seg {
    Line(V2),
    Cubic(V2, V2, V2),
}

// An SVG arc (circular, radii equal, small arc) as a cubic Bézier, the one
// thing the Phosphor paths carry that lines do not.
fn arc(from: V2, to: V2, r: f64, sweep: bool) -> Seg {
    // Centre form: the centre sits perpendicular to the chord, on the side
    // the sweep picks.
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let chord = (dx * dx + dy * dy).sqrt();
    let half = (chord / 2.0).min(r);
    let h = (r * r - half * half).max(0.0).sqrt();
    let (ux, uy) = if chord > 0.0 { (dx / chord, dy / chord) } else { (1.0, 0.0) };
    // y-down: sweep=1 bows to the left of the chord's direction.
    let sign = if sweep { -1.0 } else { 1.0 };
    let cx = (from.x + to.x) / 2.0 + uy * h * sign;
    let cy = (from.y + to.y) / 2.0 - ux * h * sign;

    let a0 = (from.y - cy).atan2(from.x - cx);
    let a1 = (to.y - cy).atan2(to.x - cx);
    let mut delta = a1 - a0;
    // y-down: sweep=1 travels with increasing angle (clockwise on screen).
    if sweep {
        if delta < 0.0 {
            delta += std::f64::consts::TAU;
        }
    } else if delta > 0.0 {
        delta -= std::f64::consts::TAU;
    }
    if delta.abs() < 1e-6 {
        delta = std::f64::consts::TAU / 4.0 * if sweep { 1.0 } else { -1.0 };
    }
    let k = 4.0 / 3.0 * (delta / 4.0).tan();
    let (c0, s0) = (a0.cos(), a0.sin());
    let (c1a, s1a) = (a1.cos(), a1.sin());
    let c1 = v(cx + r * (c0 - k * s0), cy + r * (s0 + k * c0));
    let c2 = v(cx + r * (c1a + k * s1a), cy + r * (s1a - k * c1a));
    Seg::Cubic(c1, c2, to)
}

fn play_segments() -> Vec<Seg> {
    // M240,128 a15.74 → (232.4,141.51) L(88.32,229.65) a16 → (72.12,229.95)
    // A15.86 → (64,216.13) V39.87 a15.86 → (72.12,26.05) a16 → (88.32,26.35)
    // L(232.4,114.49) A15.74 → (240,128) Z — in Phosphor's 256 grid.
    vec![
        Seg::Line(v(240.0, 128.0)),
        arc(v(240.0, 128.0), v(232.4, 141.51), 15.74, true),
        Seg::Line(v(88.32, 229.65)),
        arc(v(88.32, 229.65), v(72.12, 229.95), 16.0, true),
        arc(v(72.12, 229.95), v(64.0, 216.13), 15.86, true),
        Seg::Line(v(64.0, 39.87)),
        arc(v(64.0, 39.87), v(72.12, 26.05), 15.86, true),
        arc(v(72.12, 26.05), v(88.32, 26.35), 16.0, true),
        Seg::Line(v(232.4, 114.49)),
        arc(v(232.4, 114.49), v(240.0, 128.0), 15.74, true),
    ]
}

fn stop_segments() -> Vec<Seg> {
    // M216,56 V200 a16 → (200,216) H56 a16 → (40,200) V56 a16 → (56,40)
    // H200 a16 → (216,56) Z — a rounded square, centred in the 256 grid.
    vec![
        Seg::Line(v(216.0, 56.0)),
        Seg::Line(v(216.0, 200.0)),
        arc(v(216.0, 200.0), v(200.0, 216.0), 16.0, true),
        Seg::Line(v(56.0, 216.0)),
        arc(v(56.0, 216.0), v(40.0, 200.0), 16.0, true),
        Seg::Line(v(40.0, 56.0)),
        arc(v(40.0, 56.0), v(56.0, 40.0), 16.0, true),
        Seg::Line(v(200.0, 40.0)),
        arc(v(200.0, 40.0), v(216.0, 56.0), 16.0, true),
    ]
}

// Build the mark as a fill path, scaled from the 256 grid and centred in
// the circle the way PlayMark.qml centres its Shape.
pub fn build_mark(
    running: bool,
    circle: (f64, f64, f64), // x, y, size of the play circle
) -> Option<gpui::Path<gpui::Pixels>> {
    use gpui::{point, px, PathBuilder};

    let (cx, cy, w) = circle;
    let box_px = mark_box(w);
    let scale = box_px / 256.0;
    let origin_x = cx - box_px / 2.0;
    let origin_y = cy - box_px / 2.0;

    let segments = if running { stop_segments() } else { play_segments() };
    let mut b = PathBuilder::fill();
    let mut first = true;
    for seg in segments {
        match seg {
            Seg::Line(p) => {
                if first {
                    b.move_to(point(px((origin_x + p.x * scale) as f32), px((origin_y + p.y * scale) as f32)));
                    first = false;
                } else {
                    b.line_to(point(px((origin_x + p.x * scale) as f32), px((origin_y + p.y * scale) as f32)));
                }
            }
            Seg::Cubic(c1, c2, to) => {
                if first {
                    b.move_to(point(px((origin_x + to.x * scale) as f32), px((origin_y + to.y * scale) as f32)));
                    first = false;
                } else {
                    b.cubic_bezier_to(
                        point(px((origin_x + c1.x * scale) as f32), px((origin_y + c1.y * scale) as f32)),
                        point(px((origin_x + c2.x * scale) as f32), px((origin_y + c2.y * scale) as f32)),
                        point(px((origin_x + to.x * scale) as f32), px((origin_y + to.y * scale) as f32)),
                    );
                }
            }
        }
    }
    b.close();
    b.build().ok()
}
