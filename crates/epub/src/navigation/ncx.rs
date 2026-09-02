use alloc::{string::String, vec::Vec};
use xmlparser::{ElementEnd, Token, Tokenizer};

use crate::{
    ArchivePath,
    error::NavigationError,
    navigation::{EntryBuilder, Navigation, NavigationTarget, close_entry},
    xml::{decode_xml_value, push_decoded_xml_text},
};

enum StartTag {
    Other,
    NavMap,
    Content { src: Option<String> },
    LabelText,
}

struct LabelCapture {
    depth: usize,
    label: String,
}

struct NcxParser {
    path: ArchivePath,
    depth: usize,
    nav_map_depth: Option<usize>,
    start: StartTag,

    entries: Vec<super::NavigationEntry>,
    stack: Vec<EntryBuilder>,
    label: Option<LabelCapture>,
}

impl NcxParser {
    fn new(path: ArchivePath) -> Self {
        Self {
            path,
            depth: 0,
            nav_map_depth: None,
            start: StartTag::Other,
            entries: Vec::new(),
            stack: Vec::new(),
            label: None,
        }
    }

    fn element_start(&mut self, name: &str) {
        self.depth = self.depth.saturating_add(1);

        self.start = if name == "navMap" {
            StartTag::NavMap
        } else if self.nav_map_depth.is_some() {
            match name {
                "navPoint" => {
                    self.stack.push(EntryBuilder::default());
                    StartTag::Other
                }
                "content" if !self.stack.is_empty() => StartTag::Content { src: None },
                "text" if !self.stack.is_empty() && self.label.is_none() => StartTag::LabelText,
                _ => StartTag::Other,
            }
        } else {
            StartTag::Other
        };
    }

    fn attribute(&mut self, name: &str, value: &str) {
        if let StartTag::Content { src } = &mut self.start
            && name == "src"
        {
            *src = Some(decode_xml_value(value));
        }
    }

    fn element_boundary(&mut self, empty: bool) -> Result<(), NavigationError> {
        let start = core::mem::replace(&mut self.start, StartTag::Other);

        match start {
            StartTag::NavMap if !empty => {
                self.nav_map_depth = Some(self.depth);
            }
            StartTag::Content { src: Some(src) } => {
                let target =
                    NavigationTarget::resolve(&self.path, &src).map_err(NavigationError::Path)?;

                if let Some(entry) = self.stack.last_mut() {
                    entry.set_target(target);
                }
            }
            StartTag::LabelText if !empty => {
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
        let Some(label) = &mut self.label else {
            return;
        };

        if cdata {
            label.label.push_str(text);
        } else {
            push_decoded_xml_text(&mut label.label, text);
        }
    }

    fn close_element(&mut self, name: &str) {
        match name {
            "text" => {
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
            "navPoint" if self.nav_map_depth.is_some() => {
                close_entry(&mut self.stack, &mut self.entries);
            }
            "navMap" if self.nav_map_depth == Some(self.depth) => {
                self.nav_map_depth = None;

                self.label = None;

                self.stack.clear();
            }

            _ => {}
        }
    }

    fn finish(self) -> Navigation {
        Navigation {
            entries: self.entries,
        }
    }
}

pub(crate) fn parse_ncx(xml: &str, path: ArchivePath) -> Result<Navigation, NavigationError> {
    let mut parser = NcxParser::new(path);

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
                parser.close_element(local.as_str());
                parser.depth = parser.depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    Ok(parser.finish())
}
