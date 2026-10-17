use alloc::{string::String, vec::Vec};

use crate::{
    ArchivePath,
    error::NavigationError,
    navigation::{EntryBuilder, Navigation, NavigationEntry, NavigationTarget, close_entry},
    xml::{XmlParser, decode_xml_value, parse_xml, push_decoded_xml_text},
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
}

impl XmlParser for NavParser {
    type Error = NavigationError;

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
                if let Ok(target) = NavigationTarget::resolve(&self.path, &href)
                    && let Some(entry) = self.stack.last_mut()
                {
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

                        // an entry whose target doesn't resolve keeps its label
                        if let Some(href) = capture.href
                            && let Ok(target) = NavigationTarget::resolve(&self.path, &href)
                        {
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

    fn element_end(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

impl NavParser {
    fn finish(self) -> Navigation {
        Navigation {
            entries: self.entries,
        }
    }
}

pub(crate) fn parse_nav(xml: &str, path: ArchivePath) -> Result<Navigation, NavigationError> {
    let mut parser = NavParser::new(path);

    parse_xml(xml, &mut parser, NavigationError::Xml)?;

    Ok(parser.finish())
}
