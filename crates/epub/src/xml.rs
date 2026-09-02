use alloc::string::String;

pub(crate) fn decode_xml_value(value: &str) -> String {
    let mut output = String::with_capacity(value.len());

    push_decoded_xml_text(&mut output, value);

    output
}

pub(crate) fn push_decoded_xml_text(output: &mut String, input: &str) {
    let mut remaining = input;

    while let Some(start) = remaining.find('&') {
        output.push_str(&remaining[..start]);

        let after = &remaining[start + 1..];
        let Some(end) = after.find(';') else {
            output.push_str(&remaining[start..]);

            return;
        };

        let entity = &after[..end];

        if let Some(character) = decode_entity(entity) {
            output.push(character);
        } else {
            // keep unknown/custom entities verbatim.
            // we can add DTD entity resolution if a real EPUB proves that we need it.
            output.push('&');
            output.push_str(entity);
            output.push(';');
        }

        remaining = &after[end + 1..];
    }

    output.push_str(remaining);
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let value = if let Some(hex) = entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
            {
                u32::from_str_radix(hex, 16).ok()?
            } else if let Some(decimal) = entity.strip_prefix('#') {
                decimal.parse::<u32>().ok()?
            } else {
                return None;
            };

            char::from_u32(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_text_decodes_standard_and_numeric_entities() {
        assert_eq!(
            decode_xml_value("Fish &amp; Chips &#x2014; &#169;",),
            "Fish & Chips — ©",
        );
    }

    #[test]
    fn xml_text_preserves_unknown_entities() {
        assert_eq!(
            decode_xml_value("hello &publisher; world",),
            "hello &publisher; world",
        );
    }
}
