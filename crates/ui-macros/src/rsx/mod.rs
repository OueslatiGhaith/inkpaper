use proc_macro2::{Span, TokenStream};
use quote::quote;
use rstml::{
    Parser, ParserConfig,
    node::{Node, NodeElement},
};
use syn::Ident;

use crate::rsx::control_flow::ControlFlow;

mod attribute;
mod class;
mod component;
mod control_flow;
mod element;
mod spec;

#[cfg(test)]
mod tests;

pub(super) type RsxNode = Node<ControlFlow>;
pub(super) type RsxElement = NodeElement<ControlFlow>;

pub(crate) fn expand_rsx(input: TokenStream) -> syn::Result<TokenStream> {
    let nodes =
        Parser::new(ParserConfig::new().custom_node::<ControlFlow>()).parse_simple(input)?;

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

    let cx = Ident::new("__inkpaper_rsx_cx", Span::mixed_site());

    let body = match root {
        Node::Element(element) => element::expand_mount_element(element, None, &cx)?,
        Node::Custom(control_flow) => control_flow::expand_mount_control_flow(control_flow, &cx)?,
        Node::Fragment(_) => {
            return Err(syn::Error::new_spanned(
                root,
                "an rsx! fragment cannot be the root because InkPaper requires one mounted root element",
            ));
        }
        _ => {
            return Err(syn::Error::new_spanned(
                root,
                "the root of rsx! must be an element or a control-flow block",
            ));
        }
    };

    Ok(quote! {
        ::inkpaper_ui::MountFn::new(
            move |#cx: &mut ::inkpaper_ui::MountCx<'_>| -> ::core::result::Result<::inkpaper_ui::NodeId, ::inkpaper_ui::MountError> {
                #body
            }
        )
    })
}
