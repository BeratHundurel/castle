use gpui::{Hsla, Rgba};

const MINIMUM_TEXT_CONTRAST: f32 = 4.5;

pub(crate) struct AccessibleTextColors {
    pub(crate) background: Hsla,
    pub(crate) foreground: Hsla,
}

pub(crate) fn accessible_text_colors(
    background: Hsla,
    preferred_foreground: Hsla,
) -> AccessibleTextColors {
    let background = opaque(background);
    let preferred_foreground = opaque(preferred_foreground);

    let foreground = if contrast_ratio(background, preferred_foreground) >= MINIMUM_TEXT_CONTRAST {
        preferred_foreground
    } else if contrast_ratio(background, Hsla::black()) >= contrast_ratio(background, Hsla::white())
    {
        Hsla::black()
    } else {
        Hsla::white()
    };

    AccessibleTextColors {
        background,
        foreground,
    }
}

fn opaque(color: Hsla) -> Hsla {
    Hsla { a: 1.0, ..color }
}

fn contrast_ratio(first: Hsla, second: Hsla) -> f32 {
    let first = relative_luminance(first);
    let second = relative_luminance(second);
    let lighter = first.max(second);
    let darker = first.min(second);

    (lighter + 0.05) / (darker + 0.05)
}

fn relative_luminance(color: Hsla) -> f32 {
    let Rgba { r, g, b, .. } = color.to_rgb();

    0.2126 * linear_channel(r) + 0.7152 * linear_channel(g) + 0.0722 * linear_channel(b)
}

fn linear_channel(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use gpui::hsla;

    use super::*;

    #[test]
    fn keeps_a_theme_foreground_that_meets_the_contrast_target() {
        let colors = accessible_text_colors(Hsla::white(), Hsla::black());

        assert_eq!(colors.background, Hsla::white());
        assert_eq!(colors.foreground, Hsla::black());
    }

    #[test]
    fn replaces_a_low_contrast_theme_foreground() {
        let background = hsla(0.0, 0.0, 0.9, 0.4);
        let colors = accessible_text_colors(background, hsla(0.0, 0.0, 0.8, 0.5));

        assert_eq!(colors.background.a, 1.0);
        assert_eq!(colors.foreground, Hsla::black());
        assert!(contrast_ratio(colors.background, colors.foreground) >= MINIMUM_TEXT_CONTRAST);
    }

    #[test]
    fn fallback_meets_the_target_across_the_color_space() {
        for hue_step in 0..12 {
            for saturation_step in 0..=4 {
                for lightness_step in 0..=100 {
                    let background = hsla(
                        hue_step as f32 / 12.0,
                        saturation_step as f32 / 4.0,
                        lightness_step as f32 / 100.0,
                        1.0,
                    );
                    let colors = accessible_text_colors(background, background);

                    assert!(
                        contrast_ratio(colors.background, colors.foreground)
                            >= MINIMUM_TEXT_CONTRAST
                    );
                }
            }
        }
    }
}
