//! The macOS accent colour, made safe for white text: the UI puts white text
//! on `--accent` fills (spec §7), and a yellow or light green accent would
//! make it unreadable.

/// WCAG's minimum contrast for user-interface components.
const MIN_CONTRAST: f64 = 3.0;

fn linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn contrast_with_white((r, g, b): (u8, u8, u8)) -> f64 {
    let luminance = 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
    1.05 / (luminance + 0.05)
}

/// `rgb` as `#rrggbb`, darkened in 10% steps until white text on it has at
/// least 3:1 contrast. Black always passes, so this ends.
pub fn accent_hex(mut rgb: (u8, u8, u8)) -> String {
    while contrast_with_white(rgb) < MIN_CONTRAST {
        let darker = |c: u8| (u16::from(c) * 9 / 10) as u8;
        rgb = (darker(rgb.0), darker(rgb.1), darker(rgb.2));
    }
    format!("#{:02x}{:02x}{:02x}", rgb.0, rgb.1, rgb.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(hex: &str) -> (u8, u8, u8) {
        let n = |i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
        (n(1), n(3), n(5))
    }

    #[test]
    fn a_dark_enough_accent_is_kept() {
        assert_eq!(accent_hex((10, 100, 216)), "#0a64d8");
        assert_eq!(accent_hex((0, 0, 0)), "#000000");
    }

    #[test]
    fn a_light_accent_is_darkened_until_white_text_is_readable() {
        for rgb in [(255, 204, 0), (152, 255, 152), (255, 255, 255)] {
            let fixed = parse(&accent_hex(rgb));
            assert!(
                contrast_with_white(fixed) >= 3.0,
                "{rgb:?} became {fixed:?}"
            );
        }
    }

    #[test]
    fn darkening_keeps_the_hue() {
        let (r, g, b) = parse(&accent_hex((255, 204, 0)));
        assert!(r > g && g > b, "yellow stays yellow: {r} {g} {b}");
    }
}
