pub const BASE_PX: i32 = 4;

pub fn resolve_pixels(value: &str) -> Option<i32> {
    if value == "px" {
        return Some(1);
    }

    let (numerator, denominator) = parse_non_negative_decimal(value)?;

    let scaled = numerator.checked_mul(i64::from(BASE_PX))?;

    if scaled % denominator != 0 {
        return None;
    }

    i32::try_from(scaled / denominator).ok()
}

fn parse_non_negative_decimal(value: &str) -> Option<(i64, i64)> {
    if value.is_empty() {
        return None;
    }

    let mut numerator = 0i64;
    let mut denominator = 1i64;
    let mut seen_decimal = false;
    let mut seen_digit = false;

    for byte in value.bytes() {
        match byte {
            b'0'..=b'9' => {
                seen_digit = true;

                numerator = numerator
                    .checked_mul(10)?
                    .checked_add(i64::from(byte - b'0'))?;

                if seen_decimal {
                    denominator = denominator.checked_mul(10)?;
                }
            }

            b'.' if !seen_decimal => {
                seen_decimal = true;
            }

            _ => return None,
        }
    }

    seen_digit.then_some((numerator, denominator))
}

#[cfg(test)]
mod tests {
    use super::resolve_pixels;

    #[test]
    fn resolves_tailwind_spacing() {
        assert_eq!(resolve_pixels("0"), Some(0));
        assert_eq!(resolve_pixels("0.25"), Some(1));
        assert_eq!(resolve_pixels("0.5"), Some(2));
        assert_eq!(resolve_pixels("1"), Some(4));
        assert_eq!(resolve_pixels("1.5"), Some(6));
        assert_eq!(resolve_pixels("4"), Some(16));
        assert_eq!(resolve_pixels("17"), Some(68));
        assert_eq!(resolve_pixels("px"), Some(1));
    }

    #[test]
    fn rejects_spacing_that_cannot_be_represented_in_integer_pixels() {
        assert_eq!(resolve_pixels("0.125"), None);
        assert_eq!(resolve_pixels("-1"), None);
        assert_eq!(resolve_pixels("foo"), None);
    }
}
