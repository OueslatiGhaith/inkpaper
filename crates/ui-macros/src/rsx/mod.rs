use inkpaper_ui_style_schema::tailwind::{ValueKind, border, color, radius, spacing, typography};
use proc_macro2::{Literal, Span, TokenStream};
use quote::quote;
use rstml::{
    Infallible,
    node::{Node, NodeAttribute, NodeBlock, NodeElement},
    parse2,
};
use syn::{Expr, Ident, Lit, Stmt};

use crate::rsx::spec::{ArgumentKind, UtilityReceiver, UtilitySpec, utility_specs};

mod spec;

enum ClassValue<'a> {
    Tailwind(&'a str),
    Arbitrary(&'a str),
    Rust(&'a str),
}

pub(crate) fn expand_rsx(input: TokenStream) -> syn::Result<TokenStream> {
    let nodes = parse2(input)?;

    let root = match nodes.as_slice() {
        [] => {
            return Err(syn::Error::new(
                Span::call_site(),
                "rsx! requires exactly one root element",
            ));
        }
        [root] => root,
        [_, second, ..] => {
            return Err(syn::Error::new_spanned(
                second,
                "rsx! requires exactly one root element",
            ));
        }
    };

    match root {
        Node::Element(element) => expand_element(element),
        _ => Err(syn::Error::new_spanned(
            root,
            "the root of rsx! must be an element",
        )),
    }
}

fn expand_element(element: &NodeElement<Infallible>) -> syn::Result<TokenStream> {
    let name = element.name().to_string();

    if name != "div" {
        return Err(syn::Error::new_spanned(
            element,
            format!("unsupported rsx! element <{name}>"),
        ));
    }

    let class = parse_class_attribute(element)?;

    let mut expression = quote! { ::inkpaper_ui::div() };

    if let Some((classes, span)) = class {
        for class in split_classes(&classes, span)? {
            expression = apply_class(expression, class, span)?;
        }
    }

    for child in element.children() {
        let child = expand_child(child)?;

        expression = quote! {
            ::inkpaper_ui::ParentElement::child(
                #expression,
                #child,
            )
        };
    }

    Ok(expression)
}

fn expand_child(node: &Node<Infallible>) -> syn::Result<TokenStream> {
    match node {
        Node::Element(element) => expand_element(element),
        Node::Block(block) => expand_block(block),
        _ => Err(syn::Error::new_spanned(node, "not supported yet")),
    }
}

fn expand_block(block: &NodeBlock) -> syn::Result<TokenStream> {
    let Some(block) = block.try_block() else {
        return Err(syn::Error::new_spanned(
            block,
            "invalid Rust expression in rsx! child",
        ));
    };

    if let [Stmt::Expr(expression, None)] = block.stmts.as_slice() {
        return Ok(quote! { #expression });
    }

    Ok(quote! { #block })
}

fn parse_class_attribute(element: &NodeElement<Infallible>) -> syn::Result<Option<(String, Span)>> {
    let mut class = None;

    for attribute in element.attributes() {
        let NodeAttribute::Attribute(attribute) = attribute else {
            return Err(syn::Error::new_spanned(
                attribute,
                "dynamic rsx! attributes are not supported yet",
            ));
        };

        let name = attribute.key.to_string();

        if name != "class" {
            return Err(syn::Error::new_spanned(
                attribute,
                format!("unsupported attribute `{name}`"),
            ));
        }

        if class.is_some() {
            return Err(syn::Error::new_spanned(
                attribute,
                "duplicate `class` attribute",
            ));
        }

        let Some(value) = attribute.value() else {
            return Err(syn::Error::new_spanned(
                attribute,
                "`class` requires a string literal value",
            ));
        };

        let Expr::Lit(expression) = value else {
            return Err(syn::Error::new_spanned(
                value,
                "`class` must be a string literal",
            ));
        };

        let Lit::Str(value) = &expression.lit else {
            return Err(syn::Error::new_spanned(
                value,
                "`class` must be a string literal",
            ));
        };

        class = Some((value.value(), value.span()));
    }

    Ok(class)
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
            '{' => {
                brace_depth += 1;
            }
            '}' => {
                if brace_depth == 0 {
                    return Err(syn::Error::new(span, "unmatched `}` in class attribute"));
                }

                brace_depth -= 1;
            }
            '[' if brace_depth == 0 => {
                bracket_depth += 1;
            }
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

fn apply_class(receiver: TokenStream, class: &str, span: Span) -> syn::Result<TokenStream> {
    let specs = utility_specs();

    if let Some(spec) = specs
        .iter()
        .find(|spec| spec.argument_kind() == ArgumentKind::None && spec.classname() == class)
    {
        return Ok(emit_no_argument_utility(receiver, spec, span));
    }

    let mut candidates = Vec::new();

    for spec in &specs {
        if spec.argument_types.is_empty() {
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

        return Ok(ClassValue::Rust(value));
    }

    Ok(ClassValue::Tailwind(value))
}

fn emit_no_argument_utility(receiver: TokenStream, spec: &UtilitySpec, span: Span) -> TokenStream {
    let method = Ident::new(spec.rust_name, span);

    match spec.receiver {
        UtilityReceiver::Styled => quote! { ::inkpaper_ui::Styled::#method(#receiver) },
        UtilityReceiver::TextStyled => quote! { ::inkpaper_ui::TextStyled::#method(#receiver) },
    }
}

fn emit_argument_utility(
    receiver: TokenStream,
    spec: &UtilitySpec,
    class_name: &str,
    value: &str,
    span: Span,
) -> syn::Result<TokenStream> {
    if spec.argument_types.len() != 1 {
        return Err(syn::Error::new(
            span,
            format!(
                "utility class `{class_name}` cannot be represented by class syntax because it takes {} arguments",
                spec.argument_types.len(),
            ),
        ));
    }

    match parse_class_value(value, span)? {
        ClassValue::Rust(expression) => emit_rust_value(receiver, spec, expression, span),
        ClassValue::Arbitrary(value) => {
            emit_arbitrary_value(receiver, spec, class_name, value, span)
        }
        ClassValue::Tailwind(value) => emit_tailwind_value(receiver, spec, class_name, value, span),
    }
}

fn emit_rust_value(
    receiver: TokenStream,
    spec: &UtilitySpec,
    expression: &str,
    span: Span,
) -> syn::Result<TokenStream> {
    let expression = syn::parse_str::<Expr>(expression).map_err(|error| {
        syn::Error::new(
            span,
            format!("invalid Rust expression in class value: {error}"),
        )
    })?;

    Ok(emit_utility_call(
        receiver,
        spec,
        quote! { #expression },
        span,
    ))
}

fn emit_arbitrary_value(
    receiver: TokenStream,
    spec: &UtilitySpec,
    class_name: &str,
    value: &str,
    span: Span,
) -> syn::Result<TokenStream> {
    if spec.value_kind == Some(ValueKind::Color) {
        let rgb = color::parse_hex(value).ok_or_else(|| {
            syn::Error::new(
                span,
                format!("arbitrary color `{class_name}-[{value}]` must be `#rgb` or `#rrggbb`"),
            )
        })?;

        return Ok(emit_color_utility(receiver, spec, rgb, span));
    }

    match spec.argument_kind() {
        ArgumentKind::Pixels => {
            let value = parse_arbitrary_pixels(class_name, value, span)?;
            let value = Literal::i32_unsuffixed(value);

            Ok(emit_utility_call(
                receiver,
                spec,
                quote! { ::inkpaper_ui::px(#value) },
                span,
            ))
        }
        ArgumentKind::U16 => {
            let value = value.parse::<u16>().map_err(|_| {
                syn::Error::new(span, format!("utility class `{class_name}` requires a u16"))
            })?;

            let value = Literal::u16_unsuffixed(value);

            Ok(emit_utility_call(receiver, spec, quote! { #value }, span))
        }
        ArgumentKind::Unsupported => Err(syn::Error::new(
            span,
            format!("arbitrary values are not supported for `{class_name}`"),
        )),
        ArgumentKind::None => unreachable!("argument utility unexpectedly had no arguments"),
    }
}

fn emit_tailwind_value(
    receiver: TokenStream,
    spec: &UtilitySpec,
    class_name: &str,
    value: &str,
    span: Span,
) -> syn::Result<TokenStream> {
    match spec.value_kind {
        Some(ValueKind::Spacing) => {
            let pixels = spacing::resolve_pixels(value).ok_or_else(|| {
                syn::Error::new(
                    span,
                    format!("`{class_name}-{value}` cannot be represented"),
                )
            })?;

            emit_pixel_utility(receiver, spec, pixels, span)
        }
        Some(ValueKind::BorderWidth) => {
            let pixels = border::resolve_pixels(value).ok_or_else(|| {
                syn::Error::new(
                    span,
                    format!("invalid Tailwind border width `{class_name}-{value}`"),
                )
            })?;

            emit_pixel_utility(receiver, spec, pixels, span)
        }
        Some(ValueKind::Radius) => {
            let pixels = radius::resolve_pixels(value).ok_or_else(|| {
                syn::Error::new(
                    span,
                    format!("unknown Tailwind radius `{class_name}-{value}`"),
                )
            })?;

            emit_pixel_utility(receiver, spec, pixels, span)
        }
        Some(ValueKind::Integer) => {
            let value = value.parse::<u16>().map_err(|_| {
                syn::Error::new(span, format!("`{class_name}-{value}` requires a u16"))
            })?;

            let value = Literal::u16_unsuffixed(value);

            Ok(emit_utility_call(receiver, spec, quote! { #value }, span))
        }
        Some(ValueKind::FontSize) => {
            let font_size = typography::font_size::resolve(value).ok_or_else(|| {
                syn::Error::new(
                    span,
                    format!("unknown Tailwind font size `{class_name}-{value}`"),
                )
            })?;

            Ok(emit_font_size_utility(receiver, spec, font_size, span))
        }
        Some(ValueKind::LineHeight) => {
            let pixels = typography::line_height::resolve_pixels(value).ok_or_else(|| {
                syn::Error::new(
                    span,
                    format!(
                        "`{class_name}-{value}` cannot be represented as an integer pixel line height"
                    ),
                )
            })?;

            emit_pixel_utility(receiver, spec, pixels, span)
        }
        Some(ValueKind::Color) => {
            let value = color::resolve(value).ok_or_else(|| {
                syn::Error::new(
                    span,
                    format!("unknown Tailwind color `{class_name}-{value}`"),
                )
            })?;

            let rgb = oklch_to_srgb(value);

            Ok(emit_color_utility(receiver, spec, rgb, span))
        }

        None => match spec.argument_kind() {
            ArgumentKind::U16 => {
                let value = value.parse::<u16>().map_err(|_| {
                    syn::Error::new(
                        span,
                        format!("utility class `{class_name}` requires a u16 value"),
                    )
                })?;

                let value = Literal::u16_unsuffixed(value);

                Ok(emit_utility_call(receiver, spec, quote! { #value }, span))
            }
            _ => Err(syn::Error::new(
                span,
                format!("utility `{class_name}` has no Tailwind value scale"),
            )),
        },
    }
}

fn emit_pixel_utility(
    receiver: TokenStream,
    spec: &UtilitySpec,
    pixels: i32,
    span: Span,
) -> syn::Result<TokenStream> {
    let pixels = Literal::i32_unsuffixed(pixels);

    Ok(emit_utility_call(
        receiver,
        spec,
        quote! { ::inkpaper_ui::px(#pixels) },
        span,
    ))
}

fn emit_color_utility(
    receiver: TokenStream,
    spec: &UtilitySpec,
    rgb: color::Rgb,
    span: Span,
) -> TokenStream {
    let red = Literal::u8_unsuffixed(rgb.red);
    let green = Literal::u8_unsuffixed(rgb.green);
    let blue = Literal::u8_unsuffixed(rgb.blue);

    emit_utility_call(
        receiver,
        spec,
        quote! {
            ::inkpaper_ui::Color::rgb(
                #red,
                #green,
                #blue,
            )
        },
        span,
    )
}

fn oklch_to_srgb(value: color::Oklch) -> color::Rgb {
    let hue = value.hue.to_radians();

    let a = value.chroma * hue.cos();
    let b = value.chroma * hue.sin();

    let l_ = value.lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = value.lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = value.lightness - 0.089_484_177_5 * a - 1.291_485_548_0 * b;

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    let red_linear = 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s;
    let green_linear = -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s;
    let blue_linear = -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s;

    color::Rgb {
        red: linear_to_srgb_channel(red_linear),
        green: linear_to_srgb_channel(green_linear),
        blue: linear_to_srgb_channel(blue_linear),
    }
}

fn linear_to_srgb_channel(value: f64) -> u8 {
    let value = if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };

    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn emit_font_size_utility(
    receiver: TokenStream,
    spec: &UtilitySpec,
    font_size: typography::FontSize,
    span: Span,
) -> TokenStream {
    let size = Literal::i32_unsuffixed(font_size.size_px);
    let line_height = Literal::i32_unsuffixed(font_size.line_height_px);

    let receiver = emit_utility_call(receiver, spec, quote! { ::inkpaper_ui::px(#size) }, span);

    quote! { ::inkpaper_ui::TextStyled::line_height(#receiver, ::inkpaper_ui::px(#line_height)) }
}

fn emit_utility_call(
    receiver: TokenStream,
    spec: &UtilitySpec,
    argument: TokenStream,
    span: Span,
) -> TokenStream {
    let method = Ident::new(spec.rust_name, span);

    match spec.receiver {
        UtilityReceiver::Styled => quote! { ::inkpaper_ui::Styled::#method(#receiver, #argument) },
        UtilityReceiver::TextStyled => {
            quote! { ::inkpaper_ui::TextStyled::#method( #receiver, #argument) }
        }
    }
}

fn parse_arbitrary_pixels(class_name: &str, value: &str, span: Span) -> syn::Result<i32> {
    let Some(value) = value.strip_suffix("px") else {
        return Err(syn::Error::new(
            span,
            format!("arbitrary `{class_name}` values require the `px` unit"),
        ));
    };

    value
        .parse::<i32>()
        .map_err(|_| syn::Error::new(span, format!("invalid arbitrary pixel value `{value}px`")))
}
