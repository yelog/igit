//! Shared terminal colours and cell metrics for Git tool windows.
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

pub(crate) const PANEL: Color = Color::Rgb(28, 29, 34);
pub(crate) const INK: Color = Color::Rgb(207, 210, 219);
pub(crate) const DIM: Color = Color::Rgb(139, 143, 154);
pub(crate) const EDGE: Color = Color::Rgb(53, 55, 63);
pub(crate) const BLUE: Color = Color::Rgb(116, 164, 248);
pub(crate) const GOLD: Color = Color::Rgb(238, 190, 85);
pub(crate) const SELECT: Color = Color::Rgb(49, 70, 117);
pub(crate) const TAG: Color = Color::Rgb(48, 49, 56);
pub(crate) const CHECK: Color = Color::Rgb(98, 157, 255);
pub(crate) const CONFLICT: Color = Color::Rgb(244, 183, 105);
pub(crate) const GREEN: Color = Color::Rgb(116, 191, 122);
pub(crate) const LOCAL_REF: Color = Color::Rgb(97, 168, 112);
pub(crate) const REMOTE_REF: Color = Color::Rgb(176, 137, 222);
pub(crate) const RED: Color = Color::Rgb(239, 139, 138);

pub(crate) fn button_style(primary: bool) -> Style {
    if primary {
        Style::new()
            .fg(Color::White)
            .bg(SELECT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(INK).bg(TAG)
    }
}

pub(crate) fn checkbox(icons: super::icons::IconMode, checked: bool) -> Span<'static> {
    let style = if checked {
        Style::new().fg(CHECK).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(DIM)
    };
    Span::styled(icons.check(checked), style)
}

pub(crate) fn tracking_spans(
    icons: super::icons::IconMode,
    (ahead, behind): (usize, usize),
) -> Vec<Span<'static>> {
    use super::icons::Icon;
    [
        (Icon::Ahead, ahead, LOCAL_REF),
        (Icon::Behind, behind, BLUE),
    ]
    .into_iter()
    .filter(|(_, count, _)| *count > 0)
    .map(|(icon, count, color)| {
        Span::styled(
            format!(" {}{count}", icons.glyph(icon)),
            Style::new().fg(color),
        )
    })
    .collect()
}

pub(crate) fn diff_line(line: &str) -> Line<'_> {
    let color = if line.starts_with("diff --git ")
        || line.starts_with("index ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
    {
        DIM
    } else if line.starts_with("@@") {
        BLUE
    } else if line.starts_with('+') {
        GREEN
    } else if line.starts_with('-') {
        RED
    } else {
        INK
    };
    Line::styled(line, Style::new().fg(color))
}

pub(crate) fn repo_tint(root: usize) -> Color {
    const C: [Color; 8] = [
        Color::Rgb(31, 139, 110),
        Color::Rgb(164, 118, 198),
        Color::Rgb(198, 151, 69),
        Color::Rgb(87, 145, 199),
        Color::Rgb(181, 110, 123),
        Color::Rgb(129, 157, 87),
        Color::Rgb(78, 154, 161),
        Color::Rgb(187, 122, 91),
    ];
    C[root % C.len()]
}

pub(crate) fn clipped(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let span = Span::raw(s);
    let mut n = 0;
    let mut out = String::new();
    for g in span.styled_graphemes(Style::default()) {
        n += g.symbol.width();
        if n >= width {
            break;
        }
        out.push_str(g.symbol);
    }
    out.push('…');
    out
}

/// Keep whole trailing directories (e.g. …/lang/es_MX), or the last
/// directory's tail if necessary, without splitting a grapheme.
pub(crate) fn clipped_path(hint: String, width: usize) -> String {
    if hint.width() <= width {
        return hint;
    }
    if width == 0 {
        return String::new();
    }
    let mut left = width - 1;
    let mut start = hint.len();
    for (slash, _) in hint.rmatch_indices('/') {
        let len = hint[slash..start].width();
        if len > left {
            break;
        }
        left -= len;
        start = slash;
    }
    if start == hint.len() {
        let last = hint.rsplit('/').next().unwrap_or("");
        let span = Span::raw(last);
        let glyphs: Vec<_> = span.styled_graphemes(Style::new()).collect();
        for glyph in glyphs.iter().rev() {
            let len = glyph.symbol.width();
            if len > left {
                break;
            }
            left -= len;
            start -= glyph.symbol.len();
        }
    }
    format!("…{}", &hint[start..])
}
