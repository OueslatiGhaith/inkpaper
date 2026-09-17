use alloc::vec::Vec;

use crate::{Chapter, StyleNode, StyleNodeId};

mod parser;

#[cfg(test)]
mod tests;

use parser::{
    Declaration, DisplayValue, Property, Rule, Specificity, parse_inline_declarations,
    parse_stylesheet,
};

const FIXED_SCALE: i64 = 1_000;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CssLengthUnit {
    Pixels,
    Em,
    Percent,
}

/// a CSS length stored in thousandths of its unit.
///
/// keeping the parsed value fixed-point avoids carrying floating-point CSS values into pagination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CssLength {
    value_milli: i32,
    unit: CssLengthUnit,
}

impl CssLength {
    pub const ZERO: Self = Self {
        value_milli: 0,
        unit: CssLengthUnit::Pixels,
    };

    pub fn resolve(self, font_size: u32, percentage_basis: u32) -> i32 {
        let (basis, divisor) = match self.unit {
            CssLengthUnit::Pixels => (1i64, FIXED_SCALE),
            CssLengthUnit::Em => (i64::from(font_size), FIXED_SCALE),
            CssLengthUnit::Percent => (
                i64::from(percentage_basis),
                100i64.saturating_mul(FIXED_SCALE),
            ),
        };

        let resolved = i64::from(self.value_milli).saturating_mul(basis) / divisor;

        i32::try_from(resolved).unwrap_or(if resolved < 0 { i32::MIN } else { i32::MAX })
    }

    pub(crate) const fn pixels_milli(value_milli: i32) -> Self {
        Self {
            value_milli,
            unit: CssLengthUnit::Pixels,
        }
    }

    pub(crate) const fn em_milli(value_milli: i32) -> Self {
        Self {
            value_milli,
            unit: CssLengthUnit::Em,
        }
    }

    pub(crate) const fn percent_milli(value_milli: i32) -> Self {
        Self {
            value_milli,
            unit: CssLengthUnit::Percent,
        }
    }
}

impl Default for CssLength {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineHeightValue {
    Normal,
    Number(u32),
    Length(CssLength),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineHeight {
    value: LineHeightValue,
}

impl LineHeight {
    pub const NORMAL: Self = Self {
        value: LineHeightValue::Normal,
    };

    /// resolves an explicit CSS line-height.
    ///
    /// `None` means `normal`, in which case the text measurer's native line height should be used.
    pub fn resolve(self, font_size: u32) -> Option<u32> {
        match self.value {
            LineHeightValue::Normal => None,

            LineHeightValue::Number(value_milli) => {
                let scaled = u64::from(font_size).saturating_mul(u64::from(value_milli))
                    / u64::try_from(FIXED_SCALE).unwrap();

                Some(u32::try_from(scaled).unwrap_or(u32::MAX))
            }

            LineHeightValue::Length(length) => {
                let resolved = length.resolve(font_size, font_size).max(0);

                Some(u32::try_from(resolved).unwrap_or(u32::MAX))
            }
        }
    }

    pub(crate) const fn number_milli(value_milli: u32) -> Self {
        Self {
            value: LineHeightValue::Number(value_milli),
        }
    }

    pub(crate) const fn length(length: CssLength) -> Self {
        Self {
            value: LineHeightValue::Length(length),
        }
    }
}

impl Default for LineHeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ComputedStyle {
    font_weight: FontWeight,
    font_style: FontStyle,
    text_align: TextAlign,

    // non-inherited block properties.
    margin_top: Option<CssLength>,
    margin_bottom: Option<CssLength>,

    // inherited text properties.
    text_indent: CssLength,
    line_height: LineHeight,

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

    pub const fn margin_top(self) -> Option<CssLength> {
        self.margin_top
    }

    pub const fn margin_bottom(self) -> Option<CssLength> {
        self.margin_bottom
    }

    pub const fn text_indent(self) -> CssLength {
        self.text_indent
    }

    pub const fn line_height(self) -> LineHeight {
        self.line_height
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

    margin_top: Option<Candidate<CssLength>>,
    margin_bottom: Option<Candidate<CssLength>>,
    text_indent: Option<Candidate<CssLength>>,
    line_height: Option<Candidate<LineHeight>>,

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

        // these properties are not inherited.
        computed.hidden = false;
        computed.margin_top = None;
        computed.margin_bottom = None;

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

        if let Some(candidate) = cascade.margin_top {
            computed.margin_top = Some(candidate.value);
        }

        if let Some(candidate) = cascade.margin_bottom {
            computed.margin_bottom = Some(candidate.value);
        }

        if let Some(candidate) = cascade.text_indent {
            computed.text_indent = candidate.value;
        }

        if let Some(candidate) = cascade.line_height {
            computed.line_height = candidate.value;
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
        Property::MarginTop(value) => {
            consider_candidate(
                &mut cascade.margin_top,
                Candidate {
                    value,
                    important: declaration.important,
                    specificity,
                    order,
                },
            );
        }
        Property::MarginBottom(value) => {
            consider_candidate(
                &mut cascade.margin_bottom,
                Candidate {
                    value,
                    important: declaration.important,
                    specificity,
                    order,
                },
            );
        }
        Property::TextIndent(value) => {
            consider_candidate(
                &mut cascade.text_indent,
                Candidate {
                    value,
                    important: declaration.important,
                    specificity,
                    order,
                },
            );
        }
        Property::LineHeight(value) => {
            consider_candidate(
                &mut cascade.line_height,
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
