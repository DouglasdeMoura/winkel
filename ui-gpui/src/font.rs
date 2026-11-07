// Qt-shaped font metrics, because the pixel parity of this experiment lives
// and dies on the line metrics. The QML measures its type with FreeType the
// way Qt does: ascent and descent from hhea scaled to the pixel size at
// 1/64-px precision, and glyph ink from the unhinted outline. swash reads
// the same tables, so the numbers here land where the QML's do.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use swash::scale::ScaleContext;
use swash::{FontRef, GlyphId};

pub struct FontMetrics {
    _bytes: Vec<u8>,
    font: FontRef<'static>,
    upem: f64,
    ascent_units: f64,
    descent_units: f64,
    leading_units: f64,
    mono_advance_units: f64,
    // The digits' ink box in font units, y measured down from the baseline:
    // top is negative (above the baseline), height positive.
    digit_ink_top: f64,
    digit_ink_height: f64,
    // Swash needs a context per thread; the metrics here are computed once.
    context: std::sync::Mutex<ScaleContext>,
}

static FONT_CACHE: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();

// The file fontconfig serves for a family — the same file Qt's FreeType
// engine opens, substitution and all.
fn font_path(family: &str) -> PathBuf {
    let cache = FONT_CACHE.get_or_init(|| {
        let mut map = HashMap::new();
        if let Ok(out) = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", family])
            .output()
        {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                map.insert(family.to_string(), PathBuf::from(path));
            }
        }
        map
    });
    cache.get(family).cloned().unwrap_or_default()
}

impl FontMetrics {
    pub fn resolve(family: &str) -> Option<FontMetrics> {
        let path = font_path(family);
        let bytes = std::fs::read(&path).ok()?;
        // The Vec owns the bytes; the FontRef borrows them for the struct's
        // life. Safe because the Vec is never resized or moved out.
        let font = FontRef::from_index(unsafe { std::mem::transmute::<&[u8], &'static [u8]>(&*bytes) }, 0)?;
        Some(Self::from_font(font, bytes))
    }

    fn from_font(font: FontRef<'static>, bytes: Vec<u8>) -> Self {
        let metrics = font.metrics(&[]);
        let upem = metrics.units_per_em as f64;
        // Monospace advance: the first digit's advance covers them all, and
        // "0" is where the QML's probe starts too.
        let charmap = font.charmap();
        let zero = charmap.map('0');
        let glyph_metrics = font.glyph_metrics(&[]);
        let mono_advance_units = glyph_metrics.advance_width(zero) as f64;

        // The digits' ink box, in font units, from the unhinted outlines.
        // Outline coordinates run y-up; the QML's run y-down, so the ink
        // top the QML sees is negative and measured from the baseline.
        let mut ctx = ScaleContext::new();
        let mut scaler = ctx
            .builder(font)
            .size(upem as f32) // 1 px per font unit: bounds come back in font units
            .hint(false)
            .build();
        let mut top = f64::NEG_INFINITY;
        let mut bottom = f64::INFINITY;
        for ch in '0'..='9' {
            let gid = charmap.map(ch);
            if let Some(outline) = scaler.scale_outline(gid) {
                let b = outline.bounds();
                top = top.max(b.max.y as f64);
                bottom = bottom.min(b.min.y as f64);
            }
        }
        if !top.is_finite() || !bottom.is_finite() {
            // A font without digit outlines: fall back to the em box.
            top = metrics.ascent as f64 * 0.75;
            bottom = -0.2;
        }
        let digit_ink_top = -top;
        let digit_ink_height = top - bottom;

        FontMetrics {
            _bytes: bytes,
            font,
            upem,
            ascent_units: metrics.ascent as f64,
            descent_units: metrics.descent as f64, // swash's descent is already positive
            leading_units: metrics.leading as f64,
            mono_advance_units,
            digit_ink_top,
            digit_ink_height,
            context: std::sync::Mutex::new(ctx),
        }
    }

    // FreeType scales a font-unit metric to a pixel size by rounding to
    // 1/64 px: FT_MulFix(ascender, y_scale). Qt reads that value straight
    // into QFontMetricsF, so the same rounding lives here.
    fn ft_scaled(&self, units: f64, size: f64) -> f64 {
        let raw = units * size / self.upem;
        (raw * 64.0).round() / 64.0
    }

    pub fn ascent(&self, size: f64) -> f64 {
        self.ft_scaled(self.ascent_units, size)
    }

    pub fn descent(&self, size: f64) -> f64 {
        self.ft_scaled(self.descent_units, size)
    }

    // QFontMetricsF::height(): ascent + descent + leading.
    pub fn line_height(&self, size: f64) -> f64 {
        self.ascent(size) + self.descent(size) + self.ft_scaled(self.leading_units, size)
    }

    pub fn mono_advance(&self, size: f64) -> f64 {
        self.ft_scaled(self.mono_advance_units, size)
    }

    // The digits' ink: top offset from the baseline (negative, above it)
    // and the height, at the given pixel size.
    pub fn digit_ink(&self, size: f64) -> (f64, f64) {
        (
            self.ft_scaled(self.digit_ink_top, size),
            self.ft_scaled(self.digit_ink_height, size),
        )
    }

    // A glyph id for a char, for painting single glyphs by hand.
    pub fn glyph_id(&self, ch: char) -> Option<GlyphId> {
        Some(self.font.charmap().map(ch))
    }

    pub fn swash_font(&self) -> FontRef<'static> {
        self.font
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jetbrains_mono_metrics_are_sane() {
        let Some(fm) = FontMetrics::resolve("JetBrainsMono Nerd Font") else {
            eprintln!("fc-match could not find the font; skipping");
            return;
        };
        // JetBrains Mono's hhea: 1020 up, 300 down per 1000 em, no gap.
        let a = fm.ascent(100.0);
        let d = fm.descent(100.0);
        assert!((95.0..=110.0).contains(&a), "ascent {}", a);
        assert!((25.0..=35.0).contains(&d), "descent {}", d);
        let (top, h) = fm.digit_ink(100.0);
        assert!(top < -60.0 && top > -80.0, "digit ink top {}", top);
        assert!((65.0..=80.0).contains(&h), "digit ink height {}", h);
        assert!((55.0..=65.0).contains(&fm.mono_advance(100.0)));
    }
}
