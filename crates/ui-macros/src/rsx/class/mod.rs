use inkpaper_ui_style_schema::tailwind::ValueKind;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Ident;

use super::{
    attribute::ClassAttribute,
    spec::{ArgumentKind, UtilityReceiver, UtilitySpec, utility_specs},
};

mod color;
mod emit;

use emit::{emit_argument_utility, emit_no_argument_utility};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClassTarget {
    Div,
    Text,
    Image,
}

impl ClassTarget {
    fn accepts(self, spec: &UtilitySpec) -> bool {
        match self {
            Self::Div => matches!(
                spec.receiver,
                UtilityReceiver::Styled | UtilityReceiver::TextStyled
            ),
            Self::Text => spec.receiver == UtilityReceiver::TextStyled,
            Self::Image => spec.receiver == UtilityReceiver::Image,
        }
    }

    pub(super) fn tag_name(self) -> &'static str {
        match self {
            Self::Div => "div",
            Self::Text => "text",
            Self::Image => "image",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InteractionVariant {
    Focus,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RustValueHint {
    Color,
    Length,
}

impl RustValueHint {
    fn accepts(self, value_kind: Option<ValueKind>) -> bool {
        match self {
            Self::Color => value_kind == Some(ValueKind::Color),
            Self::Length => matches!(
                value_kind,
                Some(ValueKind::Spacing)
                    | Some(ValueKind::BorderWidth)
                    | Some(ValueKind::Radius)
                    | Some(ValueKind::FontSize)
                    | Some(ValueKind::LineHeight)
            ),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Length => "length",
        }
    }
}

enum ClassValue<'a> {
    Tailwind(&'a str),
    Arbitrary(&'a str),
    Rust {
        hint: Option<RustValueHint>,
        expression: &'a str,
    },
}

fn split_interaction_variant(class: &str) -> (Option<InteractionVariant>, &str) {
    if let Some(class) = class.strip_prefix("focus:") {
        return (Some(InteractionVariant::Focus), class);
    }
    if let Some(class) = class.strip_prefix("active:") {
        return (Some(InteractionVariant::Active), class);
    }

    (None, class)
}

pub(super) fn apply_classes(
    mut receiver: TokenStream,
    class: ClassAttribute,
    target: ClassTarget,
) -> syn::Result<TokenStream> {
    if let Some((classes, span)) = class {
        for class in split_classes(&classes, span)? {
            let (variant, _) = split_interaction_variant(class);
            if variant.is_some() {
                continue;
            }

            receiver = apply_class(receiver, class, span, target)?;
        }
    }

    Ok(receiver)
}

pub(super) fn has_interaction_variants(class: &ClassAttribute) -> syn::Result<bool> {
    let Some((classes, span)) = class else {
        return Ok(false);
    };

    for class in split_classes(classes, *span)? {
        if split_interaction_variant(class).0.is_some() {
            return Ok(true);
        }
    }

    Ok(false)
}

pub(super) fn apply_interaction_classes(
    mut receiver: TokenStream,
    class: ClassAttribute,
    target: ClassTarget,
) -> syn::Result<TokenStream> {
    let Some((classes, span)) = class else {
        return Ok(receiver);
    };

    for class in split_classes(&classes, span)? {
        let (variant, utility) = split_interaction_variant(class);
        let Some(variant) = variant else {
            continue;
        };

        if target != ClassTarget::Div {
            return Err(syn::Error::new(
                span,
                format!(
                    "interaction variants are not supported on <{}>",
                    target.tag_name(),
                ),
            ));
        }

        if utility.is_empty() {
            return Err(syn::Error::new(
                span,
                format!("interaction variant `{class}` requires a utility"),
            ));
        }

        if split_interaction_variant(utility).0.is_some() {
            return Err(syn::Error::new(
                span,
                format!("stacked interaction variants are not supported in `{class}`"),
            ));
        }

        let style = Ident::new("__inkpaper_ui_interaction_style", Span::mixed_site());
        let transformed = apply_class(quote! { #style }, utility, span, target)?;

        receiver = match variant {
            InteractionVariant::Focus => {
                quote! { ::inkpaper_ui::StatefulInteractiveElementExt::when_focused(#receiver, |#style| #transformed) }
            }
            InteractionVariant::Active => {
                quote! { ::inkpaper_ui::StatefulInteractiveElementExt::when_pressed(#receiver, |#style| #transformed) }
            }
        };
    }

    Ok(receiver)
}

fn apply_class(
    receiver: TokenStream,
    class: &str,
    span: Span,
    target: ClassTarget,
) -> syn::Result<TokenStream> {
    let specs = utility_specs();

    if let Some(spec) = specs.iter().find(|spec| {
        target.accepts(spec)
            && spec.argument_kind() == ArgumentKind::None
            && spec.classname() == class
    }) {
        return Ok(emit_no_argument_utility(receiver, spec, span));
    }

    let mut candidates = Vec::new();

    for spec in &specs {
        if !target.accepts(spec) || spec.argument_types.is_empty() {
            continue;
        }

        let classname = spec.classname();
        let prefix = format!("{classname}-");

        let Some(value) = class.strip_prefix(&prefix) else {
            continue;
        };

        candidates.push((spec, classname, value));
    }

    if candidates.is_empty() {
        if target == ClassTarget::Image {
            return Err(syn::Error::new(
                span,
                format!("unknown <image> utility class `{class}`"),
            ));
        }

        let known_for_other_target = specs.iter().any(|spec| class_matches_spec(spec, class));

        if known_for_other_target {
            return Err(syn::Error::new(
                span,
                format!(
                    "utility class `{class}` is not valid on <{}>",
                    target.tag_name(),
                ),
            ));
        }

        return Err(syn::Error::new(
            span,
            format!("unknown utility class `{class}`"),
        ));
    }

    let longest_prefix = candidates
        .iter()
        .map(|(_, classname, _)| classname.len())
        .max()
        .unwrap();

    candidates.retain(|(_, classname, _)| classname.len() == longest_prefix);

    let mut successes = Vec::new();
    let mut errors = Vec::new();

    for (spec, classname, value) in &candidates {
        match emit_argument_utility(receiver.clone(), spec, classname, value, span) {
            Ok(tokens) => successes.push((*spec, tokens)),
            Err(error) => errors.push(error),
        }
    }

    match successes.len() {
        1 => Ok(successes.remove(0).1),
        0 if candidates.len() == 1 => Err(errors.remove(0)),
        0 => Err(syn::Error::new(
            span,
            format!("no utility in `{class}` accepts this value"),
        )),
        _ => {
            let methods = successes
                .iter()
                .map(|(spec, _)| spec.rust_name)
                .collect::<Vec<_>>()
                .join(", ");

            Err(syn::Error::new(
                span,
                format!("ambiguous utility class `{class}`. It could refer to: {methods}"),
            ))
        }
    }
}

fn class_matches_spec(spec: &UtilitySpec, class: &str) -> bool {
    let classname = spec.classname();

    if spec.argument_types.is_empty() {
        return classname == class;
    }

    class.starts_with(&format!("{classname}-"))
}

fn split_classes(classes: &str, span: Span) -> syn::Result<Vec<&str>> {
    let mut result = Vec::new();
    let mut start = None;

    let mut brace_depth = 0usize;
    let mut bracket_depth = 0usize;

    for (index, character) in classes.char_indices() {
        if character.is_ascii_whitespace() && brace_depth == 0 && bracket_depth == 0 {
            if let Some(start_index) = start.take() {
                result.push(&classes[start_index..index]);
            }

            continue;
        }

        if start.is_none() {
            start = Some(index);
        }

        match character {
            '{' => brace_depth += 1,
            '}' => {
                if brace_depth == 0 {
                    return Err(syn::Error::new(span, "unmatched `}` in class attribute"));
                }

                brace_depth -= 1;
            }
            '[' if brace_depth == 0 => bracket_depth += 1,
            ']' if brace_depth == 0 => {
                if bracket_depth == 0 {
                    return Err(syn::Error::new(span, "unmatched `]` in class attribute"));
                }

                bracket_depth -= 1;
            }
            _ => {}
        }
    }

    if brace_depth != 0 {
        return Err(syn::Error::new(span, "unclosed `{` in class attribute"));
    }

    if bracket_depth != 0 {
        return Err(syn::Error::new(span, "unclosed `[` in class attribute"));
    }

    if let Some(start_index) = start {
        result.push(&classes[start_index..]);
    }

    Ok(result)
}

fn parse_class_value(value: &str, span: Span) -> syn::Result<ClassValue<'_>> {
    if value.starts_with('[') {
        let Some(value) = value
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        else {
            return Err(syn::Error::new(span, "malformed arbitrary class value"));
        };

        return Ok(ClassValue::Arbitrary(value));
    }

    if value.starts_with('{') {
        let Some(value) = value
            .strip_prefix('{')
            .and_then(|value| value.strip_suffix('}'))
        else {
            return Err(syn::Error::new(span, "malformed Rust class value"));
        };

        let value = value.trim();

        let (hint, expression) = if let Some(expression) = value.strip_prefix("color:") {
            (Some(RustValueHint::Color), expression.trim())
        } else if let Some(expression) = value.strip_prefix("length:") {
            (Some(RustValueHint::Length), expression.trim())
        } else {
            (None, value)
        };

        if expression.is_empty() {
            return Err(syn::Error::new(
                span,
                "Rust class value requires an expression",
            ));
        }

        return Ok(ClassValue::Rust { hint, expression });
    }

    Ok(ClassValue::Tailwind(value))
}
