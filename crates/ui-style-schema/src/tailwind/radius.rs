pub fn resolve_pixels(value: &str) -> Option<i32> {
    match value {
        "none" => Some(0),
        "xs" => Some(2),
        "sm" => Some(4),
        "md" => Some(6),
        "lg" => Some(8),
        "xl" => Some(12),
        "2xl" => Some(16),
        "3xl" => Some(24),
        "4xl" => Some(32),
        "full" => Some(i32::MAX),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_pixels;

    #[test]
    fn resolves_tailwind_radius_scale() {
        assert_eq!(resolve_pixels("none"), Some(0));
        assert_eq!(resolve_pixels("xs"), Some(2));
        assert_eq!(resolve_pixels("sm"), Some(4));
        assert_eq!(resolve_pixels("md"), Some(6));
        assert_eq!(resolve_pixels("lg"), Some(8));
        assert_eq!(resolve_pixels("xl"), Some(12));
        assert_eq!(resolve_pixels("2xl"), Some(16));
        assert_eq!(resolve_pixels("3xl"), Some(24));
        assert_eq!(resolve_pixels("4xl"), Some(32));
        assert_eq!(resolve_pixels("full"), Some(i32::MAX));
    }
}
