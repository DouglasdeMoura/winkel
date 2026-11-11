// The main view's geometry, computed the way Main.qml's bindings do. Every
// dimension here is a step of the golden ladder or a space() token, and the
// band gaps split the room 1 : phi : 1/phi. Kept as pure functions of the
// state and the text metrics, so the renderer only paints what this says.

use crate::theme::Theme;

pub const WINDOW_W: f64 = 377.0;
pub const WINDOW_H: f64 = 610.0;
pub const PHI: f64 = 1.618_033_988_7;

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug)]
pub struct MeterLayout {
    pub row: Rect,
    pub spacing: f64,
    pub rect_width: f64,
    pub bar_height: f64,
    pub bar_gap: f64,
    pub head_height: f64,
}

#[derive(Clone, Debug)]
pub struct MainLayout {
    pub content_x: f64,
    pub content_w: f64,
    pub content_center: f64,
    pub meters: MeterLayout,
    pub caption: Rect,
    pub minus: Rect,
    pub hero_box: Rect,
    pub hero_font_size: f64,
    pub plus: Rect,
    pub play: Rect,
    pub ts_btn: Rect,
    pub tap_btn: Rect,
    pub sub_btn: Rect,
    pub error_y: f64,
}

// The one place the layout is computed, in the order the QML's bindings
// resolve. Text metrics arrive measured (heights of the caption's line box,
// the hero digit ink, the "Tap" advance), because those are the only values
// the QML takes from the font rather than the ladder.
pub fn main_layout(
    theme: &Theme,
    win_w: f64,
    win_h: f64,
    beats: u32,
    caption_line_h: f64,
    hero_ink_probe_h: f64, // the digits' ink height at font size 100
    hero_ink_h: f64,       // the digits' ink height at hero_font_size
    hero_digit_advance: f64,
    tap_implicit_w: f64,
) -> MainLayout {
    let phi = PHI;
    let margin = theme.golden(4);
    let room = win_h.min(theme.golden(9));
    let top_pad = margin + (win_h - room).max(0.0) / 2.0;

    let content_w = win_w.min(theme.space(320.0));
    let content_x = (win_w - content_w) / 2.0;
    let content_center = content_x + content_w / 2.0;

    // --- the meters row ---
    let spacing = if beats > 12 { theme.golden(-1) } else { theme.golden(1) };
    let rect_width = theme
        .golden(2)
        .min(((theme.space(320.0) - (beats as f64 - 1.0) * spacing) / beats as f64).floor());
    let bar_width = rect_width;
    let bar_height = round_js(bar_width / phi);
    let bar_gap = theme.golden(-1);
    let head_height = theme.golden(-2);
    let meters_h = 3.0 * bar_height + 3.0 * bar_gap + head_height;
    let meters_w = beats as f64 * rect_width + (beats as f64 - 1.0) * spacing;
    let meters = MeterLayout {
        row: Rect {
            x: content_center - meters_w / 2.0,
            y: top_pad,
            w: meters_w,
            h: meters_h,
        },
        spacing,
        rect_width,
        bar_height,
        bar_gap,
        head_height,
    };

    // --- the band gaps: the height left over splits 1 : phi : 1/phi ---
    let caption_h = caption_line_h;
    let tempo_group_h = caption_h + theme.golden(0) + hero_ink_h.max(theme.golden(3));
    let play_h = theme.golden(5);
    let button_row_h = theme.golden(3);
    let bands = meters_h + tempo_group_h + play_h + button_row_h;
    let share = (room - 2.0 * margin - bands).max(0.0) / (1.0 + phi + 1.0 / phi);
    let gap_beats = theme.golden(2).max(round_js(share));
    let gap_reach = theme.golden(3).max(round_js(share * phi));
    let gap_transport = theme.golden(1).max(round_js(share / phi));

    // --- the tempo group ---
    let tempo_y = top_pad + meters_h + gap_beats;
    let caption = Rect { x: content_x, y: tempo_y, w: content_w, h: caption_h };
    let stepper_row_y = tempo_y + caption_h + theme.golden(0);
    let stepper_row_h = hero_ink_h.max(theme.golden(3));
    let hero_box_w = hero_box_w_from_advance(hero_digit_advance);
    let stepper_row_w = theme.golden(3) + theme.golden(1) + hero_box_w + theme.golden(1) + theme.golden(3);
    let stepper_row_x = content_center - stepper_row_w / 2.0;

    let hero_box_h = round_js(hero_ink_h);
    let minus = Rect {
        x: stepper_row_x,
        y: stepper_row_y + (stepper_row_h - theme.golden(3)) / 2.0,
        w: theme.golden(3),
        h: theme.golden(3),
    };
    let hero_box = Rect {
        x: stepper_row_x + theme.golden(3) + theme.golden(1),
        y: stepper_row_y,
        w: hero_box_w,
        h: hero_box_h,
    };
    let plus = Rect {
        x: stepper_row_x + stepper_row_w - theme.golden(3),
        y: stepper_row_y + (stepper_row_h - theme.golden(3)) / 2.0,
        w: theme.golden(3),
        h: theme.golden(3),
    };

    // --- the transport band ---
    let transport_y = tempo_y + tempo_group_h + gap_reach;
    let play = Rect {
        x: content_center - play_h / 2.0,
        y: transport_y,
        w: play_h,
        h: play_h,
    };
    let button_row_y = transport_y + play_h + gap_transport;
    let tap_w = theme.golden(4).max(tap_implicit_w);
    let button_row_w = theme.golden(4) + theme.golden(1) + tap_w + theme.golden(1) + theme.golden(4);
    let button_row_x = content_center - button_row_w / 2.0;
    let ts_btn = Rect { x: button_row_x, y: button_row_y, w: theme.golden(4), h: theme.golden(3) };
    let tap_btn = Rect { x: button_row_x + theme.golden(4) + theme.golden(1), y: button_row_y, w: tap_w, h: theme.golden(3) };
    let sub_btn = Rect {
        x: button_row_x + button_row_w - theme.golden(4),
        y: button_row_y,
        w: theme.golden(4),
        h: theme.golden(3),
    };

    // --- the error line: under the buttons, one golden(1) below, outside
    // the column so its arrival never moves a band ---
    let error_y = transport_y + play_h + gap_transport + button_row_h + theme.golden(1);

    let hero_font_size = hero_font_size(theme, hero_ink_probe_h);

    MainLayout {
        content_x,
        content_w,
        content_center,
        meters,
        caption,
        minus,
        hero_box,
        hero_font_size,
        plus,
        play,
        ts_btn,
        tap_btn,
        sub_btn,
        error_y,
    }
}

pub fn round_js(v: f64) -> f64 {
    v.round()
}

// The hero numeral's box: room for four digits, so the steppers hold still
// while the number changes. Math.round(4 * digitInk.advanceWidth / 10),
// where digitInk's advanceWidth is the whole probe "0123456789" — ten mono
// advances — so the /10 leaves four of them.
pub fn hero_box_w_from_advance(digit_advance: f64) -> f64 {
    round_js(4.0 * digit_advance)
}

// The numeral is sized by its ink: the digits stand golden(4) tall. A probe
// at 100 says how tall the face's digits are per pixel of size.
pub fn hero_font_size(theme: &Theme, ink_probe_h: f64) -> f64 {
    round_js(theme.golden(4) * 100.0 / ink_probe_h.max(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The QML's numbers at its defaults: scale 1, base size 12, the window
    // 377x610, four beats, and font-metric inputs measured off the real
    // thing. The ladder values are the assertion here.
    #[test]
    fn the_default_layout_lands_on_the_golden_ladder() {
        let mut theme = Theme::new(std::path::PathBuf::from("/nonexistent"));
        theme.spacing_scale = 1.0;
        theme.base_size = 12.0;
        let layout = main_layout(&theme, WINDOW_W, WINDOW_H, 4, 14.0, 71.0, 55.0, 46.0, 60.0);
        assert_eq!(layout.meters.row.y, 56.0); // golden(4)=55 + (610-608)/2
        assert_eq!(layout.meters.rect_width, 21.0);
        assert_eq!(layout.meters.bar_height, 13.0); // round(21/phi)
        assert_eq!(layout.meters.head_height, 3.0);
        assert_eq!(layout.meters.row.h, 57.0);
        assert_eq!(layout.meters.spacing, 13.0);
        assert_eq!(layout.caption.y, 187.0); // the beats gap lands on 74
        assert_eq!(layout.play.y, 384.0); // the reach gap lands on 120
        assert_eq!(layout.play.w, 89.0);
        assert_eq!(layout.ts_btn.y, 519.0); // the transport gap lands on 46
        assert_eq!(layout.ts_btn.w, 55.0);
        assert_eq!(layout.error_y, layout.ts_btn.y + layout.ts_btn.h + 13.0);
    }
}
