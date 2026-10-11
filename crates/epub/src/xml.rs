use alloc::string::String;

use xmlparser::{ElementEnd, Token, Tokenizer};

mod entities;

/// A parser that [`parse_xml`] feeds one token at a time.
pub(crate) trait XmlParser {
    type Error;

    fn element_start(&mut self, name: &str);

    fn attribute(&mut self, name: &str, value: &str);

    fn text(&mut self, text: &str, cdata: bool);

    /// The end of a start tag; `empty` when the element closes itself.
    fn element_boundary(&mut self, empty: bool) -> Result<(), Self::Error>;

    fn close_element(&mut self, name: &str) -> Result<(), Self::Error>;

    /// Once an element has ended, after `element_boundary` for an empty one and
    /// `close_element` for the others.
    fn element_end(&mut self) {}
}

pub(crate) fn parse_xml<P: XmlParser>(
    xml: &str,
    parser: &mut P,
    xml_error: fn(xmlparser::Error) -> P::Error,
) -> Result<(), P::Error> {
    for token in Tokenizer::from(xml) {
        match token.map_err(xml_error)? {
            Token::ElementStart { local, .. } => parser.element_start(local.as_str()),
            Token::Attribute { local, value, .. } => {
                parser.attribute(local.as_str(), value.as_str());
            }
            Token::Text { text } => parser.text(text.as_str(), false),
            Token::Cdata { text, .. } => parser.text(text.as_str(), true),
            Token::ElementEnd {
                end: ElementEnd::Open,
                ..
            } => parser.element_boundary(false)?,
            Token::ElementEnd {
                end: ElementEnd::Empty,
                ..
            } => {
                parser.element_boundary(true)?;
                parser.element_end();
            }
            Token::ElementEnd {
                end: ElementEnd::Close(_, local),
                ..
            } => {
                parser.close_element(local.as_str())?;
                parser.element_end();
            }
            _ => {}
        }
    }

    Ok(())
}

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
            // entities declared in an internal DTD subset are not resolved.
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
        _ if !entity.starts_with('#') => entities::lookup(entity),
        _ => {
            let value = if let Some(hex) = entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
            {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                let decimal = entity.strip_prefix('#')?;
                decimal.parse::<u32>().ok()?
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

    #[test]
    fn xml_text_decodes_xhtml_named_entities() {
        assert_eq!(
            decode_xml_value("&copy; &mdash; &eacute;&Eacute; &hellip; &euro;"),
            "© — éÉ … €",
        );
    }

    #[test]
    fn xml_text_named_entities_are_case_sensitive() {
        assert_eq!(decode_xml_value("&COPY;"), "&COPY;");
    }
}
