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
        for class in classes.split_ascii_whitespace() {
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

fn apply_class(receiver: TokenStream, class: &str, span: Span) -> syn::Result<TokenStream> {
    let specs = utility_specs();

    if let Some(spec) = specs
        .iter()
        .find(|spec| spec.argument_kind() == ArgumentKind::None && spec.class_name() == class)
    {
        return Ok(emit_no_argument_utility(receiver, spec, span));
    }

    if let Some(spec) = specs
        .iter()
        .find(|spec| !spec.argument_types.is_empty() && spec.class_name() == class)
    {
        let class_name = spec.class_name();

        return Err(syn::Error::new(
            span,
            format!("utility class `{class_name}` requires a value"),
        ));
    }

    let mut best_match: Option<(&UtilitySpec, String, &str)> = None;

    for spec in &specs {
        if spec.argument_types.is_empty() {
            continue;
        }

        let class_name = spec.class_name();
        let prefix = format!("{class_name}-");

        let Some(value) = class.strip_prefix(&prefix) else {
            continue;
        };

        let should_replace = best_match
            .as_ref()
            .is_none_or(|(_, current_name, _)| class_name.len() > current_name.len());

        if should_replace {
            best_match = Some((spec, class_name, value));
        }
    }

    if let Some((spec, class_name, value)) = best_match {
        return emit_argument_utility(receiver, spec, &class_name, value, span);
    }

    Err(syn::Error::new(
        span,
        format!("unknown utility class `{class}`"),
    ))
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

    match spec.argument_kind() {
        ArgumentKind::Pixels => {
            let value = parse_pixel_value(class_name, value, span)?;
            let value = Literal::i32_unsuffixed(value);

            Ok(emit_utility_call(
                receiver,
                spec,
                quote! { ::inkpaper_ui::px(#value) },
                span,
            ))
        }
        ArgumentKind::U16 => {
            let value = parse_u16_value(class_name, value, span)?;
            let value = Literal::u16_unsuffixed(value);

            Ok(emit_utility_call(receiver, spec, quote! { #value }, span))
        }
        ArgumentKind::Unsupported => {
            let argument_type = spec.argument_types[0];

            Err(syn::Error::new(
                span,
                format!("utility `{class_name}` takes `{argument_type}`"),
            ))
        }
        ArgumentKind::None => unreachable!("argument utility unexpectedly had no arguments"),
    }
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

fn parse_pixel_value(class_name: &str, value: &str, span: Span) -> syn::Result<i32> {
    let value = value.parse::<i32>().map_err(|_| {
        syn::Error::new(
            span,
            format!("utility class `{class_name}` requires an integer pixel value"),
        )
    })?;

    if value < 0 {
        return Err(syn::Error::new(
            span,
            format!("utility class `{class_name}` requires a non-negative pixel value"),
        ));
    }

    Ok(value)
}

fn parse_u16_value(class_name: &str, value: &str, span: Span) -> syn::Result<u16> {
    value.parse::<u16>().map_err(|_| {
        syn::Error::new(
            span,
            format!("utility class `{class_name}` requires a u16",),
        )
    })
}
