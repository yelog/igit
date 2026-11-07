// Copyright 2000-2024 JetBrains s.r.o. and contributors.
// Rust adaptation of IDEA graph colors for IGit. Apache-2.0; see NOTICE.txt.
use ratatui::style::Color;

pub(super) fn reference_color(name: &str) -> usize {
    // Java String.hashCode uses UTF-16 code units and signed 32-bit overflow.
    name.encode_utf16().fold(0_i32, |hash, c| {
        hash.wrapping_mul(31).wrapping_add(i32::from(c))
    }) as u32 as usize
}

pub(in crate::log) fn graph_color(id: usize) -> Color {
    if id == 0 {
        // Unnamed/filter-only heads must remain readable on the terminal's dark background.
        return Color::Rgb(184, 147, 77);
    }
    // IDEA DefaultColorGenerator's reference/fragment-derived hue. Theme defaults
    // are saturation .4 and brightness .65; color is independent of screen lane.
    let id = id as i32;
    let component =
        |scale, offset| ((id.wrapping_mul(scale).wrapping_add(offset) % 100).abs() + 70) as f32;
    let rgb = [component(200, 30), component(130, 50), component(90, 100)];
    let max = rgb.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min = rgb.iter().copied().fold(f32::INFINITY, f32::min);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == rgb[0] {
        ((rgb[1] - rgb[2]) / delta).rem_euclid(6.0)
    } else if max == rgb[1] {
        (rgb[2] - rgb[0]) / delta + 2.0
    } else {
        (rgb[0] - rgb[1]) / delta + 4.0
    };
    let v = 0.65_f32;
    let c = v * 0.4;
    let x = c * (1.0 - (hue % 2.0 - 1.0).abs());
    let channels = match hue as u8 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    }
    .map(|n| ((n + v - c) * 255.0).round() as u8);
    Color::Rgb(channels[0], channels[1], channels[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_use_java_utf16_hashes_and_ideas_default_hue_mapping() {
        assert_eq!(reference_color("😀"), 1_772_899);
        assert_eq!(reference_color("main"), 3_343_801);
        assert_eq!(
            graph_color(reference_color("origin/master")),
            Color::Rgb(166, 142, 99)
        );
        assert_eq!(
            graph_color(reference_color("origin/develop")),
            Color::Rgb(115, 166, 99)
        );
    }
}
