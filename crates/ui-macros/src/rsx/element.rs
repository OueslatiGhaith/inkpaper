use proc_macro2::{Span, TokenStream};
use quote::quote;
use rstml::node::{Node, NodeBlock};
use syn::{Expr, Ident, Stmt};

use crate::rsx::{
    RsxElement, RsxNode,
    attribute::{
        EventAttributeKind, ImageAttributes, IntrinsicAttributes, SvgAttributes,
        parse_intrinsic_attributes, parse_svg_attributes,
    },
    class::{apply_interaction_classes, has_interaction_variants},
    control_flow::{expand_mount_control_flow, expand_mount_control_flow_child},
};

use super::{
    attribute::parse_image_attributes,
    class::{ClassTarget, apply_classes},
    component::{expand_component, is_component_tag},
};

/// expands an element to a `Result<NodeId, MountError>` expression mounting into `cx`
pub(super) fn expand_mount_element(
    element: &RsxElement,
    implicit_id: Option<&Expr>,
    cx: &Ident,
) -> syn::Result<TokenStream> {
    let name = element.name().to_string();

    match name.as_str() {
        "div" => expand_mount_div(element, implicit_id, cx),
        "text" => {
            let expr = expand_text(element, implicit_id)?;
            Ok(quote! { ::inkpaper_ui::Element::mount(#expr, #cx) })
        }
        "image" => {
            let expr = expand_image(element, implicit_id)?;
            Ok(quote! { ::inkpaper_ui::Element::mount(#expr, #cx) })
        }
        "svg" => {
            let expr = expand_svg(element, implicit_id)?;
            Ok(quote! { ::inkpaper_ui::Element::mount(#expr, #cx) })
        }
        _ if is_component_tag(&name) => {
            let expr = expand_component_element(element, implicit_id)?;
            Ok(quote! {
                ::inkpaper_ui::Element::mount(
                    ::inkpaper_ui::IntoElement::into_element(#expr),
                    #cx,
                )
            })
        }
        _ => Err(syn::Error::new_spanned(
            element,
            format!("unsupported rsx! element <{name}>"),
        )),
    }
}

fn expand_mount_element_as_child(
    element: &RsxElement,
    implicit_id: Option<&Expr>,
    parent: &Ident,
    cx: &Ident,
) -> syn::Result<TokenStream> {
    let name = element.name().to_string();

    match name.as_str() {
        "div" if element.children().is_empty() => {
            let expr = expand_div_shell(element, implicit_id)?;
            Ok(quote! { ::inkpaper_ui::mount_child(#parent, #expr, #cx)?; })
        }
        "div" => {
            let mount = expand_mount_div(element, implicit_id, cx)?;
            let child = Ident::new("__inkpaper_rsx_child", Span::mixed_site());
            Ok(quote! {
                {
                    let #child = #mount?;
                    ::inkpaper_ui::append_child(#parent, #child, #cx);
                }
            })
        }
        "text" => {
            let expr = expand_text(element, implicit_id)?;
            Ok(quote! { ::inkpaper_ui::mount_child(#parent, #expr, #cx)?; })
        }
        "image" => {
            let expr = expand_image(element, implicit_id)?;
            Ok(quote! { ::inkpaper_ui::mount_child(#parent, #expr, #cx)?; })
        }
        "svg" => {
            let expr = expand_svg(element, implicit_id)?;
            Ok(quote! { ::inkpaper_ui::mount_child(#parent, #expr, #cx)?; })
        }
        _ if is_component_tag(&name) => {
            let expr = expand_component_element(element, implicit_id)?;
            Ok(quote! { ::inkpaper_ui::mount_child(#parent, #expr, #cx)?; })
        }
        _ => Err(syn::Error::new_spanned(
            element,
            format!("unsupported rsx! element <{name}>"),
        )),
    }
}

fn expand_component_element(
    element: &RsxElement,
    implicit_id: Option<&Expr>,
) -> syn::Result<TokenStream> {
    let children = if element.children().is_empty() {
        None
    } else {
        let child_parent = Ident::new("__inkpaper_rsx_parent", Span::mixed_site());
        let child_cx = Ident::new("__inkpaper_rsx_cx", Span::mixed_site());
        let stmts = expand_mount_children(element.children(), &child_parent, &child_cx)?;

        Some(quote! {
            ::inkpaper_ui::ChildrenFn::new(
                |#child_parent: ::inkpaper_ui::NodeId, #child_cx: &mut ::inkpaper_ui::MountCx<'_>| -> ::core::result::Result<(), ::inkpaper_ui::MountError> {
                    #stmts
                    ::core::result::Result::Ok(())
                }
            )
        })
    };

    let expression = expand_component(element, children)?;

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

fn expand_div_shell(element: &RsxElement, implicit_id: Option<&Expr>) -> syn::Result<TokenStream> {
    let attributes = parse_intrinsic_attributes(element)?;

    apply_intrinsic_attributes(
        quote! { ::inkpaper_ui::div() },
        attributes,
        ClassTarget::Div,
        implicit_id,
        element,
    )
}

fn expand_mount_div(
    element: &RsxElement,
    implicit_id: Option<&Expr>,
    cx: &Ident,
) -> syn::Result<TokenStream> {
    let shell = expand_div_shell(element, implicit_id)?;

    if element.children().is_empty() {
        return Ok(quote! { ::inkpaper_ui::Element::mount(#shell, #cx) });
    }

    let parent = Ident::new("__inkpaper_rsx_parent", Span::mixed_site());
    let children = expand_mount_children(element.children(), &parent, cx)?;

    Ok(quote! {
        {
            let #parent = ::inkpaper_ui::Element::mount(#shell, #cx)?;
            #children
            ::core::result::Result::Ok::<::inkpaper_ui::NodeId, ::inkpaper_ui::MountError>(#parent)
        }
    })
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

fn expand_svg(element: &RsxElement, implicit_id: Option<&Expr>) -> syn::Result<TokenStream> {
    if let Some(child) = element.children().first() {
        return Err(syn::Error::new_spanned(child, "<svg> cannot have children"));
    }

    let SvgAttributes { source, intrinsic } = parse_svg_attributes(element)?;

    apply_intrinsic_attributes(
        quote! { ::inkpaper_ui::svg(#source) },
        intrinsic,
        ClassTarget::Svg,
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
        events,
    } = attributes;

    let has_activate_event = events
        .iter()
        .any(|event| matches!(event.kind, EventAttributeKind::Activate));

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
        if let Some(event) = events.first() {
            return Err(syn::Error::new_spanned(
                event.listener,
                format!("`on:{}` requires an `id` attribute", event.name),
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

        if !focusable && !has_activate_event {
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

    for event in events {
        let listener = event.listener;

        expression = match event.kind {
            EventAttributeKind::Activate => {
                quote! { ::inkpaper_ui::StatefulInteractiveElementExt::on_activate(#expression, #listener) }
            }
            EventAttributeKind::Typed(event_type) => {
                quote! { ::inkpaper_ui::StatefulInteractiveElementExt::on::<#event_type>(#expression, #listener) }
            }
        };
    }

    expression = apply_interaction_classes(expression, interaction_class, target)?;

    Ok(expression)
}

/// expands child nodes to statements mounting each into `parent` via `cx`
pub(super) fn expand_mount_children(
    children: &[RsxNode],
    parent: &Ident,
    cx: &Ident,
) -> syn::Result<TokenStream> {
    let mut statements = TokenStream::new();

    for child in children {
        statements.extend(expand_mount_child(child, parent, cx)?);
    }

    Ok(statements)
}

fn expand_mount_child(
    node: &RsxNode,
    parent: &Ident,
    cx: &Ident,
) -> syn::Result<TokenStream> {
    match node {
        Node::Fragment(fragment) => expand_mount_children(fragment.children(), parent, cx),
        Node::Custom(control_flow) => expand_mount_control_flow_child(control_flow, parent, cx),
        Node::Element(element) => expand_mount_element_as_child(element, None, parent, cx),
        Node::Block(block) => {
            let expr = expand_block(block)?;
            Ok(quote! { ::inkpaper_ui::mount_child(#parent, #expr, #cx)?; })
        }
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

/// expands a single root node (inside a root `{#if}` / `{#match}` branch) to a
/// `Result<NodeId, MountError>` expression
pub(super) fn expand_mount_node(node: &RsxNode, cx: &Ident) -> syn::Result<TokenStream> {
    match node {
        Node::Element(element) => expand_mount_element(element, None, cx),
        Node::Custom(control_flow) => expand_mount_control_flow(control_flow, cx),
        Node::Block(block) => {
            let expr = expand_block(block)?;
            Ok(quote! {
                ::inkpaper_ui::Element::mount(
                    ::inkpaper_ui::IntoElement::into_element(#expr),
                    #cx,
                )
            })
        }
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

pub(super) fn expand_mount_keyed_child(
    node: &RsxNode,
    key: &Expr,
    parent: &Ident,
    cx: &Ident,
) -> syn::Result<TokenStream> {
    match node {
        Node::Element(element) => expand_mount_element_as_child(element, Some(key), parent, cx),
        _ => {
            let mount = expand_mount_node(node, cx)?;
            let child = Ident::new("__inkpaper_rsx_child", Span::mixed_site());

            Ok(quote! {
                {
                    let #child = #mount?;
                    ::inkpaper_ui::identify_node(#child, #key, #cx)?;
                    ::inkpaper_ui::append_child(#parent, #child, #cx);
                }
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
