//! Colour arithmetic: reading and writing hex, WCAG luminance and contrast,
//! and the mix seeds grow a palette with.

use super::model::CoreThemeColor;

/// The debug magenta a role missing from both tables resolves to.
pub const UNRESOLVED: CoreThemeColor = CoreThemeColor {
    red: 1.0,
    green: 0.0,
    blue: 1.0,
    alpha: 1.0,
};

fn clamped(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// A colour with every channel clamped to 0…1 (and a non-finite one to 0).
pub fn rgba(red: f64, green: f64, blue: f64, alpha: f64) -> CoreThemeColor {
    CoreThemeColor {
        red: clamped(red),
        green: clamped(green),
        blue: clamped(blue),
        alpha: clamped(alpha),
    }
}

/// A hex digit as Swift's `Character.isHexDigit` sees one: ASCII, or its
/// fullwidth form. Only the ASCII ones carry a value; a fullwidth digit makes
/// its channel 0, as `Int(_:radix:)` failing did.
fn hex_digit(c: char) -> Option<Option<u32>> {
    match c {
        '0'..='9' | 'a'..='f' => Some(c.to_digit(16)),
        '\u{FF10}'..='\u{FF19}' | '\u{FF41}'..='\u{FF46}' => Some(None),
        _ => None,
    }
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa`, with or without the hash and with
/// surrounding whitespace ignored. Anything else is `None`.
pub fn from_hex(raw: &str) -> Option<CoreThemeColor> {
    let trimmed = raw.trim();
    let stripped = trimmed.strip_prefix('#').unwrap_or(trimmed);
    let digits: Vec<Option<u32>> = stripped
        .chars()
        .flat_map(char::to_lowercase)
        .map(hex_digit)
        .collect::<Option<Vec<_>>>()?;
    let expanded: Vec<Option<u32>> = match digits.len() {
        3 => digits.iter().flat_map(|d| [*d, *d]).collect(),
        6 | 8 => digits,
        _ => return None,
    };
    let channel = |offset: usize| -> f64 {
        match (expanded[offset], expanded[offset + 1]) {
            (Some(high), Some(low)) => f64::from(high * 16 + low) / 255.0,
            _ => 0.0,
        }
    };
    Some(rgba(
        channel(0),
        channel(2),
        channel(4),
        if expanded.len() == 8 { channel(6) } else { 1.0 },
    ))
}

fn byte(channel: f64) -> i64 {
    (channel * 255.0).round() as i64
}

/// `#RRGGBB`, or `#RRGGBBAA` when the colour is not opaque. Upper case.
pub fn hex_string(color: &CoreThemeColor) -> String {
    let base = format!(
        "#{:02X}{:02X}{:02X}",
        byte(color.red),
        byte(color.green),
        byte(color.blue)
    );
    if color.alpha < 1.0 {
        format!("{base}{:02X}", byte(color.alpha))
    } else {
        base
    }
}

/// WCAG 2.1 relative luminance of the opaque colour.
pub fn relative_luminance(color: &CoreThemeColor) -> f64 {
    fn linear(channel: f64) -> f64 {
        if channel <= 0.03928 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(color.red) + 0.7152 * linear(color.green) + 0.0722 * linear(color.blue)
}

/// WCAG 2.1 contrast ratio, 1…21, symmetric.
pub fn contrast_ratio(a: &CoreThemeColor, b: &CoreThemeColor) -> f64 {
    let first = relative_luminance(a);
    let second = relative_luminance(b);
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

/// `a` moved `t` of the way to `b`, per channel, on whole 0–255 steps,
/// rounded half away from zero. Opaque.
/// `top` laid over an opaque `bottom`, as a translucent colour is drawn.
pub fn over(top: CoreThemeColor, bottom: CoreThemeColor) -> CoreThemeColor {
    let a = top.alpha;
    rgba(
        top.red * a + bottom.red * (1.0 - a),
        top.green * a + bottom.green * (1.0 - a),
        top.blue * a + bottom.blue * (1.0 - a),
        1.0,
    )
}

pub fn mix(a: &CoreThemeColor, b: &CoreThemeColor, t: f64) -> CoreThemeColor {
    let channel = |from: f64, to: f64| {
        let start = (from * 255.0).round();
        let end = (to * 255.0).round();
        (start + (end - start) * t).round() / 255.0
    };
    rgba(
        channel(a.red, b.red),
        channel(a.green, b.green),
        channel(a.blue, b.blue),
        1.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_reads_the_three_forms_and_nothing_else() {
        let azure = from_hex("#007fff").unwrap();
        assert_eq!(hex_string(&azure), "#007FFF");
        assert_eq!(from_hex(" 007FFF\n"), Some(azure));
        assert_eq!(hex_string(&from_hex("#fff").unwrap()), "#FFFFFF");
        assert_eq!(hex_string(&from_hex("#000000b3").unwrap()), "#000000B3");
        for bad in ["", "#", "#ggg", "#12345", "#1234567", "##fff", "#fff "] {
            if bad == "#fff " {
                assert!(from_hex(bad).is_some());
                continue;
            }
            assert_eq!(from_hex(bad), None, "{bad}");
        }
        // Swift counts a fullwidth digit as a hex digit, and reads it as 0.
        assert_eq!(
            hex_string(&from_hex("#\u{FF21}\u{FF21}ffff").unwrap()),
            "#00FFFF"
        );
    }

    #[test]
    fn contrast_is_wcag() {
        let black = from_hex("#000").unwrap();
        let white = from_hex("#fff").unwrap();
        assert!((contrast_ratio(&black, &white) - 21.0).abs() < 1e-9);
        assert_eq!(
            contrast_ratio(&white, &black),
            contrast_ratio(&black, &white)
        );
    }
}
