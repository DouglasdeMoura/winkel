// The Omarchy theme, the way Theme.qml reads it: colors.toml and shell.toml
// loaded before the first paint, the one file a theme switch rewrites in
// place — theme.name — carrying the watch, and the golden ladder every
// dimension on the main view is drawn from.

use std::path::PathBuf;

pub const PHI: f64 = 1.618_033_988_7;

pub fn golden(n: i32, scale: f64) -> f64 {
    round_js(8.0 * PHI.powi(n) * scale)
}

pub fn space(px: f64, scale: f64) -> f64 {
    round_js(px * scale)
}

// JavaScript's Math.round: halves away from zero, where Rust's f64::round
// already agrees — but -0.5 and ties inside the scale make the two agree
// only when written down the same way. Kept as a name so the call sites
// read like the QML.
pub fn round_js(v: f64) -> f64 {
    v.round()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl Rgb {
    pub fn from_hex(s: &str) -> Option<Rgb> {
        let s = s.strip_prefix('#')?;
        let (r, g, b) = match s.len() {
            6 => (
                u8::from_str_radix(&s[0..2], 16).ok()?,
                u8::from_str_radix(&s[2..4], 16).ok()?,
                u8::from_str_radix(&s[4..6], 16).ok()?,
            ),
            3 => {
                let one = |x: &str| u8::from_str_radix(x, 16).ok().map(|v| v * 17);
                match (one(&s[0..1]), one(&s[1..2]), one(&s[2..3])) {
                    (Some(r), Some(g), Some(b)) => (r, g, b),
                    _ => return None,
                }
            }
            _ => return None,
        };
        Some(Rgb {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
        })
    }

    pub fn to_hex_u32(self) -> u32 {
        let c = |v: f64| ((v.clamp(0.0, 1.0) * 255.0).round() as u32) & 0xFF;
        (c(self.r) << 16) | (c(self.g) << 8) | c(self.b)
    }

    // WCAG's relative luminance, the way Theme.qml computes it.
    pub fn luminance(self) -> f64 {
        fn channel(v: f64) -> f64 {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    pub fn contrast(a: Rgb, b: Rgb) -> f64 {
        let (la, lb) = (a.luminance(), b.luminance());
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    // `ink` stepped toward `toward` until it reads at `target` against `on`.
    // The same twenty steps the QML takes, in the same sRGB space.
    pub fn readable(ink: Rgb, on: Rgb, toward: Rgb, target: f64) -> Rgb {
        if Rgb::contrast(ink, on) >= target {
            return ink;
        }
        for i in 1..=20 {
            let t = i as f64 / 20.0;
            let c = Rgb {
                r: ink.r + (toward.r - ink.r) * t,
                g: ink.g + (toward.g - ink.g) * t,
                b: ink.b + (toward.b - ink.b) * t,
            };
            if Rgb::contrast(c, on) >= target {
                return c;
            }
        }
        toward
    }

    pub fn with_alpha(self, a: f64) -> Rgba {
        Rgba { rgb: self, a }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub rgb: Rgb,
    pub a: f64,
}

impl Rgba {
    pub fn with_alpha(self, a: f64) -> Rgba {
        Rgba { rgb: self.rgb, a }
    }
}

#[derive(Clone, Debug)]
pub struct Palette {
    pub background: Rgb,
    pub foreground: Rgb,
    pub accent: Rgb,
    pub muted: Rgb,
    pub surface: Rgb,
    pub deep: Rgb,
    pub urgent: Rgb,
    pub success: Rgb,
}

impl Default for Palette {
    // The only literal colours in the UI: the fallback palette, which holds
    // the window until colors.toml answers. It reads as Dracula because that
    // palette is the shape every Omarchy theme follows.
    fn default() -> Self {
        Palette {
            background: Rgb::from_hex("#282A36").unwrap(),
            foreground: Rgb::from_hex("#F8F8F2").unwrap(),
            accent: Rgb::from_hex("#BD93F9").unwrap(),
            muted: Rgb::from_hex("#6272A4").unwrap(),
            surface: Rgb::from_hex("#21222C").unwrap(),
            deep: Rgb::from_hex("#21222C").unwrap(),
            urgent: Rgb::from_hex("#FF5555").unwrap(),
            success: Rgb::from_hex("#50FA7B").unwrap(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub palette: Palette,
    // True only once colors.toml parsed to a palette.
    pub ready: bool,
    pub theme_name: String,
    pub font_family: String,
    pub base_size: f64,
    pub spacing_scale: f64,
    pub hover_fill_alpha: f64,
    pub selected_fill_alpha: f64,
    pub corner_radius: f64,
    pub reduced_motion: bool,
    state_dir: PathBuf,
}

impl Default for Theme {
    fn default() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Theme::new(home.join(".local/state/omarchy/current"))
    }
}

impl Theme {
    pub fn new(state_dir: PathBuf) -> Self {
        let mut theme = Theme {
            palette: Palette::default(),
            ready: false,
            theme_name: String::new(),
            font_family: std::env::var("WINKEL_FONT").unwrap_or_else(|_| "JetBrainsMono Nerd Font".into()),
            base_size: 12.0,
            spacing_scale: 1.0,
            hover_fill_alpha: 0.08,
            selected_fill_alpha: 0.18,
            corner_radius: 0.0,
            reduced_motion: false,
            state_dir,
        };
        theme.reload();
        let (rounding, motion) = hyprland_queries();
        theme.corner_radius = rounding;
        theme.reduced_motion = motion;
        theme
    }

    // Re-read all three files, the way the watch handler does.
    pub fn reload(&mut self) {
        let colors_path = self.state_dir.join("theme/colors.toml");
        let shell_path = self.state_dir.join("theme/shell.toml");
        let name_path = self.state_dir.join("theme.name");

        let colors_body = std::fs::read_to_string(&colors_path).unwrap_or_default();
        let shell_body = std::fs::read_to_string(&shell_path).unwrap_or_default();
        self.theme_name = title_case(std::fs::read_to_string(&name_path).unwrap_or_default().trim());

        self.apply_colors(&colors_body);
        self.apply_shell(&shell_body);
    }

    // The theme-name file is the one a theme switch rewrites in place; the
    // QML watches it and re-reads the other two. The poller calls this.
    pub fn theme_name_mtime(&self) -> Option<std::time::SystemTime> {
        std::fs::metadata(self.state_dir.join("theme.name"))
            .and_then(|m| m.modified())
            .ok()
    }

    fn apply_colors(&mut self, body: &str) {
        let found = parse_toml(body, false);
        // The fallback is the Dracula shape every Omarchy theme follows, not
        // the palette a previous theme left: a reload lands on it afresh.
        let fb = Palette::default();
        let pick = |keys: &[&str], d: Rgb| -> Rgb {
            for (key, value) in &found {
                if keys.contains(&key.as_str()) {
                    return Rgb::from_hex(value).unwrap_or(d);
                }
            }
            d
        };
        self.palette.background = pick(&["background"], fb.background);
        self.palette.foreground = pick(&["foreground"], fb.foreground);
        self.palette.accent = pick(&["accent", "purple"], fb.accent);
        self.palette.muted = pick(&["muted", "comment"], fb.muted);
        self.palette.surface = pick(&["dark_background", "background", "selection"], fb.surface);
        self.palette.deep = pick(&["dark_background", "color0"], fb.deep);
        self.palette.urgent = pick(&["red"], fb.urgent);
        self.palette.success = pick(&["green"], fb.success);
        self.ready = !found.is_empty();
    }

    fn apply_shell(&mut self, body: &str) {
        let doc = parse_toml(body, true);
        let get = |doc: &[(String, String)], key: &str| -> Option<String> {
            doc.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
        };
        if let Some(v) = get(&doc, "font.base-size") {
            if let Ok(base) = v.parse::<f64>() {
                if base > 0.0 {
                    self.base_size = base;
                }
            }
        }
        let mut scale = 1.0;
        if let Some(v) = get(&doc, "spacing.scale") {
            if let Ok(s) = v.parse::<f64>() {
                if s > 0.0 {
                    scale = s;
                }
            }
        }
        let with_font = get(&doc, "spacing.scale-with-font").map(|v| v != "false").unwrap_or(true);
        self.spacing_scale = scale * if with_font { self.base_size / 12.0 } else { 1.0 };
        self.hover_fill_alpha = alpha_of(get(&doc, "controls.hover-cursor-fill-alpha"), 0.08);
        self.selected_fill_alpha = alpha_of(get(&doc, "controls.selected-fill-alpha"), 0.18);
    }

    // --- the tokens, as functions so every call site reads like the QML ---

    pub fn golden(&self, n: i32) -> f64 {
        golden(n, self.spacing_scale)
    }

    pub fn golden_i(&self, n: i32) -> f64 {
        self.golden(n)
    }

    pub fn space(&self, px: f64) -> f64 {
        space(px, self.spacing_scale)
    }

    pub fn caption_size(&self) -> f64 {
        round_js(self.base_size * 10.0 / 12.0)
    }

    pub fn body_small(&self) -> f64 {
        round_js(self.base_size * 11.0 / 12.0)
    }

    pub fn body(&self) -> f64 {
        self.base_size
    }

    pub fn heading(&self) -> f64 {
        round_js(self.base_size * 16.0 / 12.0)
    }

    pub fn hairline(&self) -> f64 {
        self.space(1.0)
    }

    pub fn row_padding_x(&self) -> f64 {
        self.space(12.0)
    }

    pub fn gap(&self) -> f64 {
        self.space(8.0)
    }

    pub fn hit_min(&self) -> f64 {
        24.0
    }

    pub fn ground_opacity(&self) -> f64 {
        0.5
    }

    // --- the derived colours ---

    pub fn color(&self, role: ColorRole) -> Rgba {
        let p = &self.palette;
        let rgb = match role {
            ColorRole::Background => p.background,
            ColorRole::Foreground => p.foreground,
            ColorRole::Accent => p.accent,
            ColorRole::Muted => p.muted,
            ColorRole::Surface => p.surface,
            ColorRole::Deep => p.deep,
            ColorRole::Urgent => p.urgent,
            ColorRole::Success => p.success,
            ColorRole::Line => return p.foreground.with_alpha(0.25),
            ColorRole::LineSoft => return p.foreground.with_alpha(0.12),
            ColorRole::HoverFill => return p.foreground.with_alpha(self.hover_fill_alpha),
            ColorRole::SelectedFill => return p.accent.with_alpha(self.selected_fill_alpha),
            ColorRole::Caption => {
                return Rgb::readable(p.muted, p.background, p.foreground, 4.5).with_alpha(1.0)
            }
            ColorRole::CaptionOnSurface => {
                return Rgb::readable(p.muted, p.surface, p.foreground, 4.5).with_alpha(1.0)
            }
        };
        rgb.with_alpha(1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorRole {
    Background,
    Foreground,
    Accent,
    Muted,
    Surface,
    Deep,
    Urgent,
    Success,
    Line,
    LineSoft,
    HoverFill,
    SelectedFill,
    Caption,
    CaptionOnSurface,
}

fn alpha_of(value: Option<String>, fallback: f64) -> f64 {
    let Some(v) = value else { return fallback };
    match v.trim().parse::<f64>() {
        Ok(a) if a.is_finite() => a.clamp(0.0, 1.0),
        _ => fallback,
    }
}

// The smallest TOML reader the two files call for: key = value lines,
// [sections], quoted strings, numbers, booleans; comments per line. With
// `section_mode`, sections flatten to "font.base-size" style keys.
pub fn parse_toml(body: &str, section_mode: bool) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if body.is_empty() {
        return out;
    }
    let mut section = String::new();
    for line in body.lines() {
        let mut line = match index_of_comment(line) {
            Some(hash) => &line[..hash],
            None => line,
        };
        line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            let inner = line.trim_start_matches('[').trim_end_matches(']').trim();
            section = format!("{}.", inner);
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let value = unquote(line[eq + 1..].trim());
        if !section_mode {
            section.clear();
        }
        out.push((format!("{}{}", section, key), value));
    }
    out
}

// A '#' only starts a comment outside quotes.
fn index_of_comment(line: &str) -> Option<usize> {
    let mut quoted = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '#' if !quoted => return Some(i),
            _ => {}
        }
    }
    None
}

fn unquote(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

pub fn title_case(slug: &str) -> String {
    // (^|-)([a-z]) uppercased, dashes to spaces.
    let mut out = String::new();
    let mut upper_next = true;
    for c in slug.chars() {
        if c == '-' {
            out.push(' ');
            upper_next = true;
        } else if upper_next && c.is_ascii_lowercase() {
            out.push(c.to_ascii_uppercase());
            upper_next = false;
        } else {
            out.push(c);
            upper_next = false;
        }
    }
    out
}

// Read once, not watched: the compositor's rounding and its motion setting
// are not values Winkel could disagree with twice. The QML runs hyprctl at
// load; here the same two queries answer at startup.
fn hyprland_queries() -> (f64, bool) {
    let rounding = query_hyprctl("decoration:rounding")
        .and_then(|v| v.as_i64)
        .unwrap_or(0);
    let motion = query_hyprctl("animations:enabled")
        .and_then(|v| v.as_bool)
        .unwrap_or(true);
    (rounding as f64, motion)
}

struct OptVal {
    as_i64: Option<i64>,
    as_bool: Option<bool>,
}

fn query_hyprctl(option: &str) -> Option<OptVal> {
    let out = std::process::Command::new("hyprctl")
        .args(["-j", "getoption", option])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let doc = crate::json::Json::parse(&text).ok()?;
    Some(OptVal {
        as_i64: doc.get("int").and_then(|v| v.as_f64()).map(|v| v as i64),
        as_bool: match doc.get("bool") {
            Some(crate::json::Json::Bool(b)) => Some(*b),
            _ => None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_ladder_matches_qml() {
        // Theme.golden(n) = Math.round(8 * phi^n), the ladder the QML draws
        // from: 3, 5, 8, 13, 21, 34, 55, 89.
        let cases = [
            (-2, 3.0),
            (-1, 5.0),
            (0, 8.0),
            (1, 13.0),
            (2, 21.0),
            (3, 34.0),
            (4, 55.0),
            (5, 89.0),
            (9, 608.0),
        ];
        for (n, want) in cases {
            assert_eq!(golden(n, 1.0), want, "golden({})", n);
        }
    }

    #[test]
    fn readable_lifts_muted_to_aa() {
        let muted = Rgb::from_hex("#6C664B").unwrap();
        let background = Rgb::from_hex("#FFFBEB").unwrap();
        let foreground = Rgb::from_hex("#1F1F1F").unwrap();
        let c = Rgb::readable(muted, background, foreground, 4.5);
        assert!(Rgb::contrast(c, background) >= 4.5);
        // A colour that already reads keeps itself.
        let ink = Rgb::readable(foreground, background, background, 4.5);
        assert_eq!(ink, foreground);
    }

    #[test]
    fn toml_sections_flatten() {
        let doc = parse_toml("[font]\nbase-size = 12 # px\n[spacing]\nscale = \"1.5\"\n", true);
        assert_eq!(doc.iter().find(|(k, _)| k == "font.base-size").unwrap().1, "12");
        assert_eq!(doc.iter().find(|(k, _)| k == "spacing.scale").unwrap().1, "1.5");
    }

    #[test]
    fn title_case_names_the_theme() {
        assert_eq!(title_case("tokyo-night"), "Tokyo Night");
        assert_eq!(title_case("alucard"), "Alucard");
    }
}
