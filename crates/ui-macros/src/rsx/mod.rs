use proc_macro2::{Span, TokenStream};
use rstml::{node::Node, parse2};

mod attribute;
mod class;
mod component;
mod element;
mod spec;

#[cfg(test)]
mod tests;

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
        Node::Element(element) => element::expand_element(element),
        _ => Err(syn::Error::new_spanned(
            root,
            "the root of rsx! must be an element",
        )),
    }
}
