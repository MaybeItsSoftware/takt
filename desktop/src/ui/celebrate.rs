//! The drawing half of `state::celebration`: a finished row, wrapped in an
//! animation that runs once from the moment it was finished. Every style
//! ends where it began, so the row is left as it would be without one.

use super::Tokens;
use crate::state::celebration::Style;
use gpui::{
    Animation, AnimationExt, AnyElement, Div, Hsla, IntoElement, ParentElement, Stateful, Styled,
    canvas, div, fill, point, px, relative, size,
};
use std::time::Duration;

/// `row` celebrating as `style` from `started`, or `row` as it is.
pub fn celebrate(
    row: Stateful<Div>,
    task_id: &str,
    celebrating: Option<(i64, Style)>,
    t: Tokens,
) -> AnyElement {
    let Some((started, style)) = celebrating else {
        return row.into_any_element();
    };
    let id = gpui::SharedString::from(format!("celebrate-{task_id}-{started}"));
    let seed = seed(task_id);
    let (success, accent) = (t.success, t.accent);
    row.relative()
        .with_animation(
            id,
            Animation::new(Duration::from_millis(style.duration_ms() as u64)),
            move |row, delta| match style {
                Style::Strike => strike(row, delta, success),
                Style::Spark => spark(row, delta, seed, success, accent),
                Style::Fold => fold(row, delta, success),
                Style::None => row,
            },
        )
        .into_any_element()
}

fn alpha(color: Hsla, a: f32) -> Hsla {
    Hsla {
        a: color.a * a.clamp(0., 1.),
        ..color
    }
}

fn ease_out(delta: f32) -> f32 {
    1. - (1. - delta).powi(2)
}

/// A line drawn through the row over its first three fifths, then faded,
/// with a wash of the success tint.
fn strike(row: Stateful<Div>, delta: f32, success: Hsla) -> Stateful<Div> {
    if delta >= 1. {
        return row;
    }
    let drawn = ease_out((delta / 0.6).min(1.));
    let fade = if delta < 0.6 { 1. } else { (1. - delta) / 0.4 };
    row.bg(alpha(success, 0.14 * fade)).child(
        div()
            .absolute()
            .left(px(6.))
            .top(relative(0.5))
            .h(px(1.5))
            .w(relative(drawn * 0.97))
            .bg(alpha(success, fade)),
    )
}

/// A tint over the row that folds shut about its middle.
fn fold(row: Stateful<Div>, delta: f32, success: Hsla) -> Stateful<Div> {
    if delta >= 1. {
        return row;
    }
    let open = 1. - ease_out(delta);
    row.child(
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(relative(0.5 - open / 2.))
            .h(relative(open))
            .rounded(px(4.))
            .bg(alpha(success, 0.10 + 0.18 * open)),
    )
}

/// Eighteen sparks thrown out along the row and fading, the same way every
/// time for a given row.
fn spark(row: Stateful<Div>, delta: f32, seed: u64, success: Hsla, accent: Hsla) -> Stateful<Div> {
    if delta >= 1. {
        return row;
    }
    row.child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                let eased = ease_out(delta);
                for index in 0..18 {
                    let n = noise(seed, index);
                    let x = w * (0.08 + 0.84 * n[0]) + (n[1] - 0.5) * w * 0.22 * eased;
                    let y = h * 0.5 + (n[2] - 0.5) * h * 1.6 * eased;
                    let r = (1.2 + n[3] * 1.8) * (1. - delta);
                    if r < 0.1 {
                        continue;
                    }
                    let color = if n[3] > 0.7 { accent } else { success };
                    let origin = bounds.origin + point(px(x - r), px(y - r));
                    window.paint_quad(
                        fill(
                            gpui::Bounds::new(origin, size(px(2. * r), px(2. * r))),
                            alpha(color, 1. - delta),
                        )
                        .corner_radii(px(r)),
                    );
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full(),
    )
}

fn seed(task_id: &str) -> u64 {
    task_id.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
    })
}

/// Four values in 0..1 from the seed and the spark's index, the same every
/// frame.
fn noise(seed: u64, index: u64) -> [f32; 4] {
    let mix = |salt: u64| {
        let mut v = seed ^ index.wrapping_mul(19_349_663) ^ salt.wrapping_mul(73_856_093);
        v ^= v >> 33;
        v = v.wrapping_mul(0xff51_afd7_ed55_8ccd);
        v ^= v >> 33;
        (v % 10_000) as f32 / 10_000.
    };
    [mix(1), mix(2), mix(3), mix(4)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparks_scatter_the_same_way_for_a_row_and_differently_between_rows() {
        assert_eq!(noise(seed("a"), 3), noise(seed("a"), 3));
        assert_ne!(noise(seed("a"), 3), noise(seed("b"), 3));
        assert!(noise(seed("a"), 3).iter().all(|v| (0. ..1.).contains(v)));
    }
}
