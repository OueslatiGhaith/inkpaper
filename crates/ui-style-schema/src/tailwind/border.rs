pub fn resolve_pixels(value: &str) -> Option<i32> {
    if value == "px" {
        return Some(1);
    }

    let value = value.parse::<i32>().ok()?;

    (value >= 0).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::resolve_pixels;

    #[test]
    fn resolves_border_widths_as_literal_pixels() {
        assert_eq!(resolve_pixels("0"), Some(0));
        assert_eq!(resolve_pixels("1"), Some(1));
        assert_eq!(resolve_pixels("2"), Some(2));
        assert_eq!(resolve_pixels("3"), Some(3));
        assert_eq!(resolve_pixels("8"), Some(8));
        assert_eq!(resolve_pixels("px"), Some(1));
    }
}
