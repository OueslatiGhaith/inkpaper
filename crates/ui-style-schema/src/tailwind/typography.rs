use super::spacing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontSize {
    pub size_px: i32,
    pub line_height_px: i32,
}

pub mod font_size {
    use super::FontSize;

    pub fn resolve(value: &str) -> Option<FontSize> {
        match value {
            "xs" => Some(FontSize {
                size_px: 12,
                line_height_px: 16,
            }),
            "sm" => Some(FontSize {
                size_px: 14,
                line_height_px: 20,
            }),
            "base" => Some(FontSize {
                size_px: 16,
                line_height_px: 24,
            }),
            "lg" => Some(FontSize {
                size_px: 18,
                line_height_px: 28,
            }),
            "xl" => Some(FontSize {
                size_px: 20,
                line_height_px: 28,
            }),
            "2xl" => Some(FontSize {
                size_px: 24,
                line_height_px: 32,
            }),
            "3xl" => Some(FontSize {
                size_px: 30,
                line_height_px: 36,
            }),
            "4xl" => Some(FontSize {
                size_px: 36,
                line_height_px: 40,
            }),
            "5xl" => Some(FontSize {
                size_px: 48,
                line_height_px: 48,
            }),
            "6xl" => Some(FontSize {
                size_px: 60,
                line_height_px: 60,
            }),
            "7xl" => Some(FontSize {
                size_px: 72,
                line_height_px: 72,
            }),
            "8xl" => Some(FontSize {
                size_px: 96,
                line_height_px: 96,
            }),
            "9xl" => Some(FontSize {
                size_px: 128,
                line_height_px: 128,
            }),
            _ => None,
        }
    }
}

pub mod line_height {
    use super::spacing;

    pub fn resolve_pixels(value: &str) -> Option<i32> {
        spacing::resolve_pixels(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{font_size, line_height};

    #[test]
    fn resolves_tailwind_font_sizes() {
        let sm = font_size::resolve("sm").unwrap();
        let lg = font_size::resolve("lg").unwrap();

        assert_eq!(sm.size_px, 14);
        assert_eq!(sm.line_height_px, 20);

        assert_eq!(lg.size_px, 18);
        assert_eq!(lg.line_height_px, 28);
    }

    #[test]
    fn numeric_line_heights_use_spacing_scale() {
        assert_eq!(line_height::resolve_pixels("5"), Some(20));
        assert_eq!(line_height::resolve_pixels("6"), Some(24));
        assert_eq!(line_height::resolve_pixels("7"), Some(28));
    }
}
