use alloc::{collections::BTreeMap, string::String, vec::Vec};

use crate::{StyleContext, StyleNode, StyleNodeId};

mod parser;

#[cfg(test)]
mod tests;

use parser::{
    Declaration, DisplayValue, Property, Rule, SelectorIndexKey, Specificity,
    parse_inline_declarations, parse_stylesheet,
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
    text_align: Option<TextAlign>,

    // non-inherited block properties.
    margin_top: Option<CssLength>,
    margin_bottom: Option<CssLength>,

    // inherited text properties.
    text_indent: CssLength,
    line_height: LineHeight,

    hidden: bool,
}

impl ComputedStyle {
    pub(crate) const fn with_font(font_weight: FontWeight, font_style: FontStyle) -> Self {
        Self {
            font_weight,
            font_style,
            text_align: None,
            margin_top: None,
            margin_bottom: None,
            text_indent: CssLength::ZERO,
            line_height: LineHeight::NORMAL,
            hidden: false,
        }
    }

    pub const fn font_weight(self) -> FontWeight {
        self.font_weight
    }

    pub const fn font_style(self) -> FontStyle {
        self.font_style
    }

    pub const fn text_align(self) -> TextAlign {
        match self.text_align {
            Some(align) => align,
            None => TextAlign::Start,
        }
    }

    /// the alignment the book set on this element or an ancestor, if any
    pub const fn declared_text_align(self) -> Option<TextAlign> {
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
    pub(crate) const fn from_styles(styles: Vec<ComputedStyle>) -> Self {
        Self { styles }
    }

    /// The chapter's styles from its markup alone, such as bold headings and
    /// italic `<em>`, ignoring the book's stylesheets and `style` attributes.
    /// Consumes `style_context` so the element tree is dropped once resolved.
    pub fn defaults(style_context: StyleContext) -> Self {
        resolve_chapter_styles(&style_context, None)
    }

    pub fn style(&self, node: StyleNodeId) -> Option<ComputedStyle> {
        self.styles.get(node.index()).copied()
    }
}

#[derive(Debug, Default)]
pub(crate) struct Stylesheet {
    rules: Vec<Rule>,
    declarations: Vec<Declaration>,
    next_order: usize,
    by_id: BTreeMap<String, Vec<usize>>,
    by_class: BTreeMap<String, Vec<usize>>,
    by_element: BTreeMap<String, Vec<usize>>,
    universal: Vec<usize>,
}

impl Stylesheet {
    pub(crate) fn parse(css: &str) -> Self {
        let mut stylesheet = Self::default();
        stylesheet.push(css);
        stylesheet
    }

    pub(crate) fn push(&mut self, css: &str) {
        let start = self.rules.len();
        parse_stylesheet(
            css,
            &mut self.rules,
            &mut self.declarations,
            &mut self.next_order,
        );

        for rule_index in start..self.rules.len() {
            match self.rules[rule_index].index_key() {
                SelectorIndexKey::Id(id) => {
                    self.by_id
                        .entry(String::from(id))
                        .or_default()
                        .push(rule_index);
                }
                SelectorIndexKey::Class(class) => {
                    self.by_class
                        .entry(String::from(class))
                        .or_default()
                        .push(rule_index);
                }
                SelectorIndexKey::Element(element) => {
                    self.by_element
                        .entry(String::from(element))
                        .or_default()
                        .push(rule_index);
                }
                SelectorIndexKey::Universal => {
                    self.universal.push(rule_index);
                }
            }
        }
    }

    const fn order_span(&self) -> usize {
        self.next_order
    }

    fn apply_matching_rules(
        &self,
        node: &StyleNode,
        nodes: &[StyleNode],
        order_offset: usize,
        cascade: &mut CascadedStyle,
    ) {
        if let Some(id) = node.id()
            && let Some(bucket) = self.by_id.get(id)
        {
            self.apply_bucket(bucket, node, nodes, order_offset, cascade);
        }

        let classes = node.classes();
        for (index, class) in classes.iter().enumerate() {
            if classes[..index].iter().any(|previous| previous == class) {
                continue;
            }

            if let Some(bucket) = self.by_class.get(class.as_str()) {
                self.apply_bucket(bucket, node, nodes, order_offset, cascade);
            }
        }

        let element = node.element();
        let element_bucket = if element.bytes().any(|byte| byte.is_ascii_uppercase()) {
            let lowered = element.to_ascii_lowercase();
            self.by_element.get(lowered.as_str())
        } else {
            self.by_element.get(element)
        };
        if let Some(bucket) = element_bucket {
            self.apply_bucket(bucket, node, nodes, order_offset, cascade);
        }

        if !self.universal.is_empty() {
            self.apply_bucket(&self.universal, node, nodes, order_offset, cascade);
        }
    }

    fn apply_bucket(
        &self,
        bucket: &[usize],
        node: &StyleNode,
        nodes: &[StyleNode],
        order_offset: usize,
        cascade: &mut CascadedStyle,
    ) {
        for &rule_index in bucket {
            let rule = &self.rules[rule_index];
            if !rule.matches(node, nodes) {
                continue;
            }

            let order = order_offset.saturating_add(rule.order);
            let specificity = rule.specificity();
            for &declaration in &self.declarations[rule.declarations.clone()] {
                apply_declaration(cascade, declaration, specificity, order);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Candidate<T> {
    value: T,
    rank: Rank,
}

/// How a declaration ranks in the cascade.
#[derive(Debug, Clone, Copy)]
struct Rank {
    important: bool,
    specificity: Specificity,
    order: usize,
}

impl Rank {
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

/// `None` leaves out the book's styles: its stylesheets and `style` attributes.
pub(crate) fn resolve_chapter_styles(
    style_context: &StyleContext,
    stylesheets: Option<&[&Stylesheet]>,
) -> ChapterStyles {
    let nodes = style_context.style_nodes();
    let mut styles = Vec::with_capacity(nodes.len());

    for node in nodes {
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

        if let Some(sheets) = stylesheets {
            let mut order_offset = 0usize;
            for stylesheet in sheets {
                stylesheet.apply_matching_rules(node, nodes, order_offset, &mut cascade);
                order_offset = order_offset.saturating_add(stylesheet.order_span());
            }

            if let Some(inline) = node.inline_style() {
                for declaration in parse_inline_declarations(inline) {
                    apply_declaration(&mut cascade, declaration, Specificity::INLINE, usize::MAX);
                }
            }
        }

        if let Some(candidate) = cascade.font_weight {
            computed.font_weight = candidate.value;
        }

        if let Some(candidate) = cascade.font_style {
            computed.font_style = candidate.value;
        }

        if let Some(candidate) = cascade.text_align {
            computed.text_align = Some(candidate.value);
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
    let rank = Rank {
        important: declaration.important,
        specificity,
        order,
    };

    match declaration.property {
        Property::FontWeight(value) => consider(&mut cascade.font_weight, value, rank),
        Property::FontStyle(value) => consider(&mut cascade.font_style, value, rank),
        Property::TextAlign(value) => consider(&mut cascade.text_align, value, rank),
        Property::MarginTop(value) => consider(&mut cascade.margin_top, value, rank),
        Property::MarginBottom(value) => consider(&mut cascade.margin_bottom, value, rank),
        Property::TextIndent(value) => consider(&mut cascade.text_indent, value, rank),
        Property::LineHeight(value) => consider(&mut cascade.line_height, value, rank),
        Property::Display(value) => consider(&mut cascade.display, value, rank),
    }
}

/// Keeps `value` when it outranks the candidate already in `current`.
fn consider<T>(current: &mut Option<Candidate<T>>, value: T, rank: Rank) {
    if current
        .as_ref()
        .is_none_or(|current| rank.outranks(current.rank))
    {
        *current = Some(Candidate { value, rank });
    }
}
