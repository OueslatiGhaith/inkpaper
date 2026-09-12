use proc_macro2::TokenStream;
use quote::quote;
use rstml::node::{Node, NodeBlock};
use syn::Stmt;

use crate::rsx::{
    RsxElement, RsxNode,
    control_flow::{expand_control_flow, expand_control_flow_into},
};

use super::{
    attribute::{parse_class_attribute, parse_image_attributes},
    class::{ClassTarget, apply_classes},
    component::{expand_component, is_component_tag},
};

pub(super) fn expand_element(element: &RsxElement) -> syn::Result<TokenStream> {
    let name = element.name().to_string();

    match name.as_str() {
        "div" => expand_div(element),
        "text" => expand_text(element),
        "image" => expand_image(element),

        _ if is_component_tag(&name) => expand_component(element),

        _ => Err(syn::Error::new_spanned(
            element,
            format!("unsupported rsx! element <{name}>"),
        )),
    }
}

fn expand_div(element: &RsxElement) -> syn::Result<TokenStream> {
    let class = parse_class_attribute(element)?;

    let expression = apply_classes(quote! { ::inkpaper_ui::div() }, class, ClassTarget::Div)?;

    expand_children_into(expression, element.children())
}

fn expand_text(element: &RsxElement) -> syn::Result<TokenStream> {
    let class = parse_class_attribute(element)?;

    let content = expand_text_content(element)?;

    apply_classes(
        quote! { ::inkpaper_ui::text(#content) },
        class,
        ClassTarget::Text,
    )
}

fn expand_image(element: &RsxElement) -> syn::Result<TokenStream> {
    if let Some(child) = element.children().first() {
        return Err(syn::Error::new_spanned(
            child,
            "<image> cannot have children",
        ));
    }

    let attributes = parse_image_attributes(element)?;
    let source = attributes.source;

    apply_classes(
        quote! { ::inkpaper_ui::image(#source) },
        attributes.class,
        ClassTarget::Image,
    )
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
