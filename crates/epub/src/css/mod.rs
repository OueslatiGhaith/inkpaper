use alloc::vec::Vec;

use crate::{Chapter, StyleNode, StyleNodeId};

mod parser;

#[cfg(test)]
mod tests;

use parser::{
    Declaration, DisplayValue, Property, Rule, Specificity, parse_inline_declarations,
    parse_stylesheet,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Left,
    Right,
    Center,
    Justify,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ComputedStyle {
    font_weight: FontWeight,
    font_style: FontStyle,
    text_align: TextAlign,
    hidden: bool,
}

impl ComputedStyle {
    pub const fn font_weight(self) -> FontWeight {
        self.font_weight
    }

    pub const fn font_style(self) -> FontStyle {
        self.font_style
    }

    pub const fn text_align(self) -> TextAlign {
        self.text_align
    }

    pub const fn hidden(self) -> bool {
        self.hidden
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterStyles {
    styles: Vec<ComputedStyle>,
}

impl ChapterStyles {
    pub fn style(&self, node: StyleNodeId) -> Option<ComputedStyle> {
        self.styles.get(node.index()).copied()
    }
}

#[derive(Debug, Default)]
pub(crate) struct Stylesheet {
    rules: Vec<Rule>,
    next_order: usize,
}

impl Stylesheet {
    pub(crate) fn push(&mut self, css: &str) {
        parse_stylesheet(css, &mut self.rules, &mut self.next_order);
    }
}

#[derive(Debug, Clone, Copy)]
struct Candidate<T> {
    value: T,
    important: bool,
    specificity: Specificity,
    order: usize,
}

impl<T> Candidate<T>
where
    T: Copy,
{
    fn outranks(self, current: Self) -> bool {
        if self.important != current.important {
            return self.important;
        }
        if self.specificity != current.specificity {
            return self.specificity > current.specificity;
        }

        self.order >= current.order
    }
}

#[derive(Debug, Default)]
struct CascadedStyle {
    font_weight: Option<Candidate<FontWeight>>,
    font_style: Option<Candidate<FontStyle>>,
    text_align: Option<Candidate<TextAlign>>,
    display: Option<Candidate<DisplayValue>>,
}

pub(crate) fn resolve_chapter_styles(chapter: &Chapter, stylesheet: &Stylesheet) -> ChapterStyles {
    let mut styles = Vec::with_capacity(chapter.style_nodes().len());

    for index in 0..chapter.style_nodes().len() {
        let node_id = StyleNodeId::new(index);

        let node = chapter
            .style_node(node_id)
            .expect("style node index comes from chapter style arena");

        let parent = node
            .parent()
            .and_then(|parent| styles.get(parent.index()).copied());

        let mut computed: ComputedStyle = parent.unwrap_or_default();

        // display itself is not inherited. Hidden content, however, remains hidden
        // underneath a display:none ancestor.
        let inherited_hidden = computed.hidden;

        computed.hidden = false;

        apply_semantic_defaults(node, &mut computed);

        let mut cascade = CascadedStyle::default();

        for rule in &stylesheet.rules {
            if !rule.matches(node) {
                continue;
            }

            for declaration in &rule.declarations {
                apply_declaration(&mut cascade, *declaration, rule.specificity(), rule.order);
            }
        }

        if let Some(inline) = node.inline_style() {
            for declaration in parse_inline_declarations(inline) {
                apply_declaration(&mut cascade, declaration, Specificity::INLINE, usize::MAX);
            }
        }

        if let Some(candidate) = cascade.font_weight {
            computed.font_weight = candidate.value;
        }

        if let Some(candidate) = cascade.font_style {
            computed.font_style = candidate.value;
        }

        if let Some(candidate) = cascade.text_align {
            computed.text_align = candidate.value;
        }

        let local_hidden = cascade
            .display
            .is_some_and(|candidate| candidate.value == DisplayValue::None);

        computed.hidden = inherited_hidden || local_hidden;

        styles.push(computed);
    }

    ChapterStyles { styles }
}

fn apply_semantic_defaults(node: &StyleNode, style: &mut ComputedStyle) {
    match node.element() {
        "b" | "strong" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            style.font_weight = FontWeight::Bold;
        }

        _ => {}
    }

    if matches!(node.element(), "i" | "em") {
        style.font_style = FontStyle::Italic;
    }
}

fn apply_declaration(
    cascade: &mut CascadedStyle,
    declaration: Declaration,
    specificity: Specificity,
    order: usize,
) {
    match declaration.property {
        Property::FontWeight(value) => {
            consider_candidate(
                &mut cascade.font_weight,
                Candidate {
                    value,
                    important: declaration.important,
                    specificity,
                    order,
                },
            );
        }
        Property::FontStyle(value) => {
            consider_candidate(
                &mut cascade.font_style,
                Candidate {
                    value,
                    important: declaration.important,
                    specificity,
                    order,
                },
            );
        }
        Property::TextAlign(value) => {
            consider_candidate(
                &mut cascade.text_align,
                Candidate {
                    value,
                    important: declaration.important,
                    specificity,
                    order,
                },
            );
        }
        Property::Display(value) => {
            consider_candidate(
                &mut cascade.display,
                Candidate {
                    value,
                    important: declaration.important,
                    specificity,
                    order,
                },
            );
        }
    }
}

fn consider_candidate<T>(current: &mut Option<Candidate<T>>, candidate: Candidate<T>)
where
    T: Copy,
{
    let replace = match current {
        Some(current) => candidate.outranks(*current),
        None => true,
    };

    if replace {
        *current = Some(candidate);
    }
}
