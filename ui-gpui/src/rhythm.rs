// The cell one beat is, exactly as Rhythm.js defines it: a division into 1
// to 6 slots, a pattern of the slots that tick, and a shape of the slots
// that start a figure. A figure runs from its slot to the next figure's; a
// figure whose own slot ticks is a note, one whose slot is silent a rest.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CatalogueCell {
    pub n: u32,
    pub mask: u32,
    pub shape: u32,
}

pub const CELLS: &[CatalogueCell] = &[
    CatalogueCell { n: 1, mask: 0b1, shape: 0b1 },
    CatalogueCell { n: 2, mask: 0b11, shape: 0b11 },
    CatalogueCell { n: 2, mask: 0b10, shape: 0b11 },
    CatalogueCell { n: 3, mask: 0b111, shape: 0b111 },
    CatalogueCell { n: 3, mask: 0b101, shape: 0b101 },
    CatalogueCell { n: 3, mask: 0b011, shape: 0b011 },
    CatalogueCell { n: 3, mask: 0b110, shape: 0b111 },
    CatalogueCell { n: 3, mask: 0b100, shape: 0b101 },
    CatalogueCell { n: 3, mask: 0b010, shape: 0b111 },
    CatalogueCell { n: 4, mask: 0b1111, shape: 0b1111 },
    CatalogueCell { n: 4, mask: 0b1001, shape: 0b1001 },
    CatalogueCell { n: 4, mask: 0b0011, shape: 0b0011 },
    CatalogueCell { n: 4, mask: 0b1101, shape: 0b1101 },
    CatalogueCell { n: 4, mask: 0b0111, shape: 0b0111 },
    CatalogueCell { n: 4, mask: 0b1011, shape: 0b1011 },
    CatalogueCell { n: 4, mask: 0b1110, shape: 0b1111 },
    CatalogueCell { n: 4, mask: 0b1010, shape: 0b1111 },
    CatalogueCell { n: 4, mask: 0b1100, shape: 0b1101 },
    CatalogueCell { n: 4, mask: 0b0110, shape: 0b0111 },
    CatalogueCell { n: 4, mask: 0b0010, shape: 0b0011 },
    CatalogueCell { n: 4, mask: 0b1000, shape: 0b1001 },
    CatalogueCell { n: 5, mask: 0b11111, shape: 0b11111 },
    CatalogueCell { n: 5, mask: 0b11110, shape: 0b11111 },
    CatalogueCell { n: 6, mask: 0b111111, shape: 0b111111 },
    CatalogueCell { n: 6, mask: 0b111110, shape: 0b111111 },
];

pub fn cells_for(n: u32) -> Vec<&'static CatalogueCell> {
    CELLS.iter().filter(|c| c.n == n).collect()
}

// The tiles: a grid's cells without a rest. A rest is one tap away in the
// editor's ticks, so the tiles stay the figures a player reaches for.
pub fn tiles_for(n: u32) -> Vec<&'static CatalogueCell> {
    cells_for(n).into_iter().filter(|c| c.shape == c.mask).collect()
}

// The number over a tuplet's group, 0 when the grid is a plain division.
pub fn tuplet(n: u32) -> u32 {
    match n {
        3 | 5 | 6 => n,
        _ => 0,
    }
}

// The nominal value of a grid's slot: halves and quarters for 2 and 4,
// the next power of two below the count for a tuplet.
pub fn nominal(n: u32) -> u32 {
    match n {
        3 => 2,
        n if n >= 5 => 4,
        n => n,
    }
}

// The slots that start a figure, in order.
pub fn starts(n: u32, shape: u32) -> Vec<u32> {
    (0..n).filter(|s| shape >> s & 1 == 1).collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FigureItem {
    pub rest: bool,
    pub k: f64,
    pub dot: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub n: u32,
    pub mask: u32,
    pub shape: u32,
    pub items: Vec<FigureItem>,
}

// The figure for a run of `span` slots on a grid of n: the slot's own
// value for one, twice that for two, dotted for three, four times for
// four, dotted for six, and the beat itself for the whole grid. None when
// no single figure spells the run (five sixths of a beat).
fn figure(n: u32, span: u32) -> Option<(f64, bool)> {
    let u = nominal(n) as f64;
    match span {
        s if s == n => Some((1.0, false)),
        1 => Some((u, false)),
        2 if u >= 2.0 => Some((u / 2.0, false)),
        3 if u >= 2.0 => Some((u / 2.0, true)),
        4 if u >= 4.0 => Some((u / 4.0, false)),
        6 if u >= 4.0 => Some((u / 4.0, true)),
        _ => None,
    }
}

// The cell for a division, a pattern and a shape. With no shape given, the
// catalogue's spelling of the pattern, or the literal one: a figure in
// every slot.
pub fn cell(n: u32, mask: u32, shape: Option<u32>) -> Cell {
    let mut shape = match shape {
        Some(s) => s,
        None => CELLS
            .iter()
            .find(|c| c.n == n && c.mask == mask)
            .map(|c| c.shape)
            .unwrap_or((1 << n) - 1),
    };
    shape = (shape | mask | 1) & ((1 << n) - 1);
    let at = starts(n, shape);
    let mut items = Vec::new();
    for (j, start) in at.iter().enumerate() {
        let span = if j + 1 < at.len() { at[j + 1] } else { n } - start;
        let Some((k, dot)) = figure(n, span) else {
            // A span no single figure spells falls back to a figure per slot.
            return cell(n, mask, Some((1 << n) - 1));
        };
        items.push(FigureItem { rest: mask >> start & 1 == 0, k, dot });
    }
    Cell { n, mask, shape, items }
}

pub fn figure_key(beat_value: u32, it: FigureItem) -> String {
    const VALUE_KEYS: [&str; 6] = [
        "whole", "half", "quarter", "eighth", "sixteenth", "thirtySecond",
    ];
    let index = (beat_value as f64 * it.k).log2().round();
    let key = VALUE_KEYS[(index as usize).min(VALUE_KEYS.len() - 1)];
    format!("{}{}{}", if it.rest { "rest." } else { "note." }, key, if it.dot { ".dotted" } else { "" })
}

// The spoken name of a cell: runs of one figure fold into a count ("two
// sixteenths"), a run that is the whole cell takes the "all" form
// ("sixteenths"), and a tuplet's template frames the list.
pub fn cell_name(beat_value: u32, c: &Cell, i18n: &crate::i18n::I18n) -> String {
    use crate::i18n::Arg;
    let mut parts: Vec<String> = Vec::new();
    let items = &c.items;
    let mut i = 0;
    while i < items.len() {
        let it = items[i];
        let mut run = 1;
        while i + run < items.len() && same(&items[i + run], &it) {
            run += 1;
        }
        let key = figure_key(beat_value, it);
        if run > 1 && run == items.len() {
            parts.push(i18n.tr(&key, &[("form", Arg::Form("all".into())), ("count", Arg::Count(run as f64))]));
        } else {
            parts.push(i18n.tr(&key, &[("count", Arg::Count(run as f64))]));
        }
        i += run;
    }
    let template = match c.n {
        3 => "cell.triplet",
        5 => "cell.quintuplet",
        6 => "cell.sextuplet",
        _ => "cell.plain",
    };
    let text = i18n.tr(template, &[("figures", Arg::Str(parts.join(&i18n.tr1("list.separator"))))]);
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => text,
    }
}

fn same(a: &FigureItem, b: &FigureItem) -> bool {
    a.rest == b.rest && a.k == b.k && a.dot == b.dot
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // The names the tests check below come from the en.json catalogue, the
    // same words the QML shows.
    fn catalog() -> crate::i18n::I18n {
        let mut dir = std::env::current_exe().unwrap();
        for _ in 0..6 {
            dir.pop();
            let candidate = dir.join("ui/i18n");
            if candidate.join("en.json").is_file() {
                return crate::i18n::I18n::with_dir(candidate);
            }
        }
        panic!("ui/i18n not found from the test binary");
    }

    #[test]
    fn the_dotted_eighth_and_the_split_sixteenth_tick_alike_and_spell_apart() {
        let dotted = cell(4, 0b1001, None);
        assert_eq!(dotted.shape, 0b1001);
        assert_eq!(dotted.items.len(), 2);
        assert_eq!(dotted.items[0], FigureItem { rest: false, k: 2.0, dot: true });
        assert_eq!(dotted.items[1], FigureItem { rest: false, k: 4.0, dot: false });

        let split = cell(4, 0b1001, Some(0b1111));
        assert_eq!(split.items.len(), 4);
        assert_eq!(split.items[0], FigureItem { rest: false, k: 4.0, dot: false });
        assert_eq!(split.items[1], FigureItem { rest: true, k: 4.0, dot: false });
        assert_eq!(split.items[2], FigureItem { rest: true, k: 4.0, dot: false });
        assert_eq!(split.items[3], FigureItem { rest: false, k: 4.0, dot: false });
    }

    #[test]
    fn a_rest_never_owns_the_beat() {
        // mask 0b110 on 3: the catalogue's spelling keeps a figure at 0,
        // which the mask turns into a rest; the beat itself is the rest.
        let c = cell(3, 0b110, None);
        assert_eq!(c.shape, 0b111);
        assert_eq!(c.items[0], FigureItem { rest: true, k: 2.0, dot: false });
        assert_eq!(c.items[1], FigureItem { rest: false, k: 2.0, dot: false });
        assert_eq!(c.items[2], FigureItem { rest: false, k: 2.0, dot: false });
    }

    #[test]
    fn names_read_like_the_qmls() {
        let i18n = catalog();
        // A quarter beat divided into its four slots is four sixteenths.
        assert_eq!(cell_name(4, &cell(4, 0b1111, None), &i18n), "Sixteenths");
        // A triplet's three slots are three eighths in the time of two.
        assert_eq!(cell_name(4, &cell(3, 0b111, None), &i18n), "Triplet eighths");
        assert_eq!(cell_name(4, &cell(4, 0b1001, None), &i18n), "Dotted eighth, sixteenth");
        // A run that is the whole cell takes the "all" form.
        assert_eq!(cell_name(4, &cell(2, 0b11, None), &i18n), "Eighths");
        assert_eq!(cell_name(4, &cell(1, 0b1, None), &i18n), "Quarter");
    }

    #[test]
    fn tiles_have_no_rests() {
        assert_eq!(tiles_for(1).len(), 1);
        assert_eq!(tiles_for(2).len(), 1);
        assert_eq!(tiles_for(3).len(), 3);
        assert_eq!(tiles_for(4).len(), 6);
        assert_eq!(tiles_for(5).len(), 1);
        assert_eq!(tiles_for(6).len(), 1);
    }
}
