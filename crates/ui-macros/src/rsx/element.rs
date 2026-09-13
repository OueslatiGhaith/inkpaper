use proc_macro2::{Span, TokenStream};
use quote::quote;
use rstml::node::{Node, NodeBlock};
use syn::{Expr, Stmt};

use crate::rsx::{
    RsxElement, RsxNode,
    attribute::{ImageAttributes, IntrinsicAttributes, parse_intrinsic_attributes},
    class::{apply_interaction_classes, has_interaction_variants},
    control_flow::{expand_control_flow, expand_control_flow_group_into, expand_control_flow_into},
};

use super::{
    attribute::parse_image_attributes,
    class::{ClassTarget, apply_classes},
    component::{expand_component, is_component_tag},
};

pub(super) fn expand_element(element: &RsxElement) -> syn::Result<TokenStream> {
    expand_element_with_implicit_id(element, None)
}

fn expand_element_with_implicit_id(
    element: &RsxElement,
    implicit_id: Option<&Expr>,
) -> syn::Result<TokenStream> {
    let name = element.name().to_string();

    match name.as_str() {
        "div" => expand_div(element, implicit_id),
        "text" => expand_text(element, implicit_id),
        "image" => expand_image(element, implicit_id),

        _ if is_component_tag(&name) => {
            let expression = expand_component(element)?;

            match implicit_id {
                Some(id) => Ok(quote! {
                    ::inkpaper_ui::IdentifiableElementExt::id(
                        ::inkpaper_ui::IntoElement::into_element(#expression),
                        #id,
                    )
                }),
                None => Ok(expression),
            }
        }

        _ => Err(syn::Error::new_spanned(
            element,
            format!("unsupported rsx! element <{name}>"),
        )),
    }
}

fn expand_div(element: &RsxElement, implicit_id: Option<&Expr>) -> syn::Result<TokenStream> {
    let attributes = parse_intrinsic_attributes(element)?;

    let expression = apply_intrinsic_attributes(
        quote! { ::inkpaper_ui::div() },
        attributes,
        ClassTarget::Div,
        implicit_id,
        element,
    )?;

    expand_children_into(expression, element.children())
}

fn expand_text(element: &RsxElement, implicit_id: Option<&Expr>) -> syn::Result<TokenStream> {
    let attributes = parse_intrinsic_attributes(element)?;
    let content = expand_text_content(element)?;

    apply_intrinsic_attributes(
        quote! { ::inkpaper_ui::text(#content) },
        attributes,
        ClassTarget::Text,
        implicit_id,
        element,
    )
}

fn expand_image(element: &RsxElement, implicit_id: Option<&Expr>) -> syn::Result<TokenStream> {
    if let Some(child) = element.children().first() {
        return Err(syn::Error::new_spanned(
            child,
            "<image> cannot have children",
        ));
    }

    let ImageAttributes { source, intrinsic } = parse_image_attributes(element)?;

    apply_intrinsic_attributes(
        quote! { ::inkpaper_ui::image(#source) },
        intrinsic,
        ClassTarget::Image,
        implicit_id,
        element,
    )
}

fn apply_intrinsic_attributes(
    expression: TokenStream,
    attributes: IntrinsicAttributes<'_>,
    target: ClassTarget,
    implicit_id: Option<&Expr>,
    element: &RsxElement,
) -> syn::Result<TokenStream> {
    let IntrinsicAttributes {
        class,
        id,
        focusable,
        on_activate,
    } = attributes;

    if implicit_id.is_some()
        && let Some(id) = id
    {
        return Err(syn::Error::new_spanned(
            id,
            "a keyed `{#each}` root must not declare `id`. They key already provides its identity",
        ));
    }

    let effective_id = implicit_id.or(id);
    if effective_id.is_none() {
        if let Some(listener) = on_activate {
            return Err(syn::Error::new_spanned(
                listener,
                "`on:activate` requires an `id` attribute",
            ));
        }

        if focusable {
            return Err(syn::Error::new_spanned(
                element,
                "`focusable` requires an `id` attribute",
            ));
        }
    }

    let has_variants = has_interaction_variants(&class)?;
    if has_variants {
        let span = class
            .as_ref()
            .map(|(_, span)| *span)
            .unwrap_or_else(Span::call_site);

        if target != ClassTarget::Div {
            return Err(syn::Error::new(
                span,
                format!(
                    "interaction variants are not supported on <{}>",
                    target.tag_name(),
                ),
            ));
        }

        if effective_id.is_none() {
            return Err(syn::Error::new(
                span,
                "interaction variants require an `id` attribute",
            ));
        }

        if !focusable && on_activate.is_none() {
            return Err(syn::Error::new(
                span,
                "interaction variants require `focusable` or `on:activate`",
            ));
        }
    }

    let interaction_class = class.clone();

    let mut expression = apply_classes(expression, class, target)?;

    if let Some(id) = effective_id {
        expression = quote! { ::inkpaper_ui::IdentifiableElementExt::id(#expression, #id) };
    }

    if focusable {
        expression = quote! { ::inkpaper_ui::StatefulInteractiveElementExt::focusable(#expression) }
    }

    if let Some(listener) = on_activate {
        expression = quote! { ::inkpaper_ui::StatefulInteractiveElementExt::on_activate(#expression, #listener) };
    }

    expression = apply_interaction_classes(expression, interaction_class, target)?;

    Ok(expression)
}

pub(super) fn expand_children_group(children: &[RsxNode]) -> syn::Result<TokenStream> {
    expand_children_group_into(quote! { ::inkpaper_ui::NoChildren }, children)
}

pub(super) fn expand_children_group_into(
    mut group: TokenStream,
    children: &[RsxNode],
) -> syn::Result<TokenStream> {
    for child in children {
        group = expand_child_group_into(group, child)?;
    }

    Ok(group)
}

fn expand_child_group_into(group: TokenStream, node: &RsxNode) -> syn::Result<TokenStream> {
    match node {
        Node::Fragment(fragment) => expand_children_group_into(group, fragment.children()),
        Node::Custom(control_flow) => expand_control_flow_group_into(group, control_flow),
        _ => {
            let child = expand_child(node)?;

            Ok(quote! { ::inkpaper_ui::ChildrenExt::child(#group, #child) })
        }
    }
}

pub(super) fn expand_children_into(
    mut parent: TokenStream,
    children: &[RsxNode],
) -> syn::Result<TokenStream> {
    for child in children {
        parent = expand_child_into(parent, child)?;
    }

    Ok(parent)
}

fn expand_child_into(parent: TokenStream, node: &RsxNode) -> syn::Result<TokenStream> {
    match node {
        Node::Fragment(fragment) => expand_children_into(parent, fragment.children()),
        Node::Custom(control_flow) => expand_control_flow_into(parent, control_flow),
        _ => {
            let child = expand_child(node)?;
            Ok(quote! { ::inkpaper_ui::ParentElement::child(#parent, #child) })
        }
    }
}

pub(super) fn expand_child(node: &RsxNode) -> syn::Result<TokenStream> {
    match node {
        Node::Element(element) => expand_element(element),
        Node::Custom(control_flow) => expand_control_flow(control_flow),
        Node::Block(block) => expand_block(block),
        Node::Fragment(_) => Err(syn::Error::new_spanned(
            node,
            "fragment cannot be used as a standalone element",
        )),
        Node::Text(_) => Err(syn::Error::new_spanned(
            node,
            "quoted text must be wrapped in a <text> element",
        )),
        Node::RawText(_) => Err(syn::Error::new_spanned(
            node,
            "bare text is not supported. Use <text>\"...\"</text>",
        )),
        _ => Err(syn::Error::new_spanned(node, "unsupported rsx! child")),
    }
}

pub(super) fn expand_keyed_child(node: &RsxNode, key: &Expr) -> syn::Result<TokenStream> {
    match node {
        Node::Element(element) => expand_element_with_implicit_id(element, Some(key)),
        _ => {
            let child = expand_child(node)?;

            Ok(quote! {
                ::inkpaper_ui::IdentifiableElementExt::id(
                    ::inkpaper_ui::IntoElement::into_element(#child),
                    #key,
                )
            })
        }
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

fn expand_text_content(element: &RsxElement) -> syn::Result<TokenStream> {
    match element.children() {
        [] => Err(syn::Error::new_spanned(element, "<text> requires content")),
        [Node::Text(text)] => {
            let value = &text.value;
            Ok(quote! { #value })
        }
        [Node::Block(block)] => expand_block(block),
        [child] => Err(syn::Error::new_spanned(
            child,
            "<text> content must be a quoted string or Rust expression",
        )),
        [_, second, ..] => Err(syn::Error::new_spanned(
            second,
            "<text> accepts exactly one content child",
        )),
    }
}
