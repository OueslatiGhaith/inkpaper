use proc_macro2::TokenStream;
use quote::quote;
use rstml::{
    Infallible,
    node::{Node, NodeBlock, NodeElement},
};
use syn::Stmt;

use super::{
    attribute::{parse_class_attribute, parse_image_attributes},
    class::{ClassTarget, apply_classes},
    component::{expand_component, is_component_tag},
};

pub(super) fn expand_element(element: &NodeElement<Infallible>) -> syn::Result<TokenStream> {
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

fn expand_div(element: &NodeElement<Infallible>) -> syn::Result<TokenStream> {
    let class = parse_class_attribute(element)?;

    let mut expression = apply_classes(quote! { ::inkpaper_ui::div() }, class, ClassTarget::Div)?;

    for child in element.children() {
        let child = expand_child(child)?;

        expression = quote! { ::inkpaper_ui::ParentElement::child(#expression, #child) };
    }

    Ok(expression)
}

fn expand_text(element: &NodeElement<Infallible>) -> syn::Result<TokenStream> {
    let class = parse_class_attribute(element)?;

    let content = expand_text_content(element)?;

    apply_classes(
        quote! { ::inkpaper_ui::text(#content) },
        class,
        ClassTarget::Text,
    )
}

fn expand_image(element: &NodeElement<Infallible>) -> syn::Result<TokenStream> {
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

fn expand_child(node: &Node<Infallible>) -> syn::Result<TokenStream> {
    match node {
        Node::Element(element) => expand_element(element),
        Node::Block(block) => expand_block(block),
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

fn expand_text_content(element: &NodeElement<Infallible>) -> syn::Result<TokenStream> {
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
