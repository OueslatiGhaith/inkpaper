use alloc::{string::String, vec::Vec};
use xmlparser::{ElementEnd, Token, Tokenizer};

use crate::{
    ArchivePath,
    error::NavigationError,
    navigation::{EntryBuilder, Navigation, NavigationEntry, NavigationTarget, close_entry},
    xml::{decode_xml_value, push_decoded_xml_text},
};

enum StartTag {
    Other,
    Nav { is_toc: bool },
    Anchor { href: Option<String> },
    LabelSpan,
}

struct AnchorCapture {
    depth: usize,
    href: Option<String>,
    label: String,
}

struct LabelCapture {
    depth: usize,
    label: String,
}

struct NavParser {
    path: ArchivePath,
    depth: usize,
    toc_depth: Option<usize>,
    start: StartTag,

    entries: Vec<NavigationEntry>,
    stack: Vec<EntryBuilder>,

    anchor: Option<AnchorCapture>,
    label: Option<LabelCapture>,
}

impl NavParser {
    fn new(path: ArchivePath) -> Self {
        Self {
            path,
            depth: 0,
            toc_depth: None,
            start: StartTag::Other,
            entries: Vec::new(),
            stack: Vec::new(),
            anchor: None,
            label: None,
        }
    }

    fn element_start(&mut self, name: &str) {
        self.depth = self.depth.saturating_add(1);

        self.start = if name == "nav" {
            StartTag::Nav { is_toc: false }
        } else if self.toc_depth.is_some() {
            match name {
                "li" => {
                    self.stack.push(EntryBuilder::default());
                    StartTag::Other
                }
                "a" if !self.stack.is_empty() => StartTag::Anchor { href: None },
                "span"
                    if self.anchor.is_none()
                        && self.label.is_none()
                        && self.stack.last().is_some_and(|entry| entry.label.is_none()) =>
                {
                    StartTag::LabelSpan
                }
                _ => StartTag::Other,
            }
        } else {
            StartTag::Other
        };
    }

    fn attribute(&mut self, name: &str, value: &str) {
        match &mut self.start {
            StartTag::Nav { is_toc } => {
                if name == "type" && value.split_ascii_whitespace().any(|value| value == "toc") {
                    *is_toc = true;
                }
            }

            StartTag::Anchor { href } => {
                if name == "href" {
                    *href = Some(decode_xml_value(value));
                }
            }

            StartTag::Other | StartTag::LabelSpan => {}
        }
    }

    fn element_boundary(&mut self, empty: bool) -> Result<(), NavigationError> {
        let start = core::mem::replace(&mut self.start, StartTag::Other);

        match start {
            StartTag::Nav { is_toc: true } if !empty => {
                self.toc_depth = Some(self.depth);
            }
            StartTag::Anchor { href } if !empty => {
                self.anchor = Some(AnchorCapture {
                    depth: self.depth,
                    href,
                    label: String::new(),
                });
            }

            StartTag::Anchor { href: Some(href) } => {
                let target =
                    NavigationTarget::resolve(&self.path, &href).map_err(NavigationError::Path)?;

                if let Some(entry) = self.stack.last_mut() {
                    entry.set_target(target);
                }
            }

            StartTag::LabelSpan if !empty => {
                self.label = Some(LabelCapture {
                    depth: self.depth,
                    label: String::new(),
                });
            }

            _ => {}
        }

        Ok(())
    }

    fn text(&mut self, text: &str, cdata: bool) {
        if let Some(anchor) = &mut self.anchor {
            if cdata {
                anchor.label.push_str(text);
            } else {
                push_decoded_xml_text(&mut anchor.label, text);
            }

            return;
        }

        if let Some(label) = &mut self.label {
            if cdata {
                label.label.push_str(text);
            } else {
                push_decoded_xml_text(&mut label.label, text);
            }
        }
    }

    fn close_element(&mut self, name: &str) -> Result<(), NavigationError> {
        match name {
            "a" => {
                let should_finish = self
                    .anchor
                    .as_ref()
                    .is_some_and(|capture| capture.depth == self.depth);

                if should_finish {
                    let capture = self.anchor.take().expect("anchor capture exists");

                    if let Some(entry) = self.stack.last_mut() {
                        entry.set_label(capture.label);

                        if let Some(href) = capture.href {
                            let target = NavigationTarget::resolve(&self.path, &href)
                                .map_err(NavigationError::Path)?;

                            entry.set_target(target);
                        }
                    }
                }
            }

            "span" => {
                let should_finish = self
                    .label
                    .as_ref()
                    .is_some_and(|capture| capture.depth == self.depth);

                if should_finish {
                    let capture = self.label.take().expect("label capture exists");

                    if let Some(entry) = self.stack.last_mut() {
                        entry.set_label(capture.label);
                    }
                }
            }

            "li" if self.toc_depth.is_some() => {
                close_entry(&mut self.stack, &mut self.entries);
            }

            "nav" if self.toc_depth == Some(self.depth) => {
                self.toc_depth = None;
                self.anchor = None;
                self.label = None;
                self.stack.clear();
            }

            _ => {}
        }

        Ok(())
    }

    fn finish(self) -> Navigation {
        Navigation {
            entries: self.entries,
        }
    }
}

pub(crate) fn parse_nav(xml: &str, path: ArchivePath) -> Result<Navigation, NavigationError> {
    let mut parser = NavParser::new(path);

    for token in Tokenizer::from(xml) {
        match token.map_err(NavigationError::Xml)? {
            Token::ElementStart { local, .. } => {
                parser.element_start(local.as_str());
            }
            Token::Attribute { local, value, .. } => {
                parser.attribute(local.as_str(), value.as_str());
            }
            Token::Text { text } => {
                parser.text(text.as_str(), false);
            }
            Token::Cdata { text, .. } => {
                parser.text(text.as_str(), true);
            }
            Token::ElementEnd {
                end: ElementEnd::Open,
                ..
            } => {
                parser.element_boundary(false)?;
            }
            Token::ElementEnd {
                end: ElementEnd::Empty,
                ..
            } => {
                parser.element_boundary(true)?;
                parser.depth = parser.depth.saturating_sub(1);
            }
            Token::ElementEnd {
                end: ElementEnd::Close(_, local),
                ..
            } => {
                parser.close_element(local.as_str())?;
                parser.depth = parser.depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    Ok(parser.finish())
}
