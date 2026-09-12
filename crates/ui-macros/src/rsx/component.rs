use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use rstml::{Infallible, node::NodeElement};
use syn::Path;

use super::attribute::parse_component_props;

pub(super) fn is_component_tag(name: &str) -> bool {
    let Ok(path) = syn::parse_str::<Path>(name) else {
        return false;
    };

    let Some(segment) = path.segments.last() else {
        return false;
    };

    segment
        .ident
        .to_string()
        .chars()
        .next()
        .is_some_and(char::is_uppercase)
}

pub(super) fn expand_component(element: &NodeElement<Infallible>) -> syn::Result<TokenStream> {
    if let Some(child) = element.children().first() {
        return Err(syn::Error::new_spanned(
            child,
            "custom component children are not supported yet",
        ));
    }

    let component = parse_component_path(element)?;
    let props_type = component_props_path(&component)?;
    let props = parse_component_props(element)?;
    let names = props.iter().map(|prop| &prop.name);
    let values = props.iter().map(|prop| prop.value);

    Ok(quote! {
        #component::from(
            #props_type {
                #( #names: #values, )*
            }
        )
    })
}

fn parse_component_path(element: &NodeElement<Infallible>) -> syn::Result<Path> {
    let name = element.name().to_string();

    syn::parse_str::<Path>(&name).map_err(|error| {
        syn::Error::new_spanned(
            element,
            format!("invalid custom component path `{name}`: {error}"),
        )
    })
}

fn component_props_path(component: &Path) -> syn::Result<Path> {
    let mut props = component.clone();

    let Some(segment) = props.segments.last_mut() else {
        return Err(syn::Error::new(
            Span::call_site(),
            "component path cannot be empty",
        ));
    };

    let name = format!("{}Props", segment.ident);

    segment.ident = format_ident!("{}", name, span = segment.ident.span(),);

    Ok(props)
}
