use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Fields, ItemStruct};

pub(crate) fn expand_component(input: TokenStream) -> syn::Result<TokenStream> {
    let component = syn::parse2::<ItemStruct>(input)?;

    let ident = &component.ident;
    let visibility = &component.vis;
    let generics = &component.generics;

    let props_ident = format_ident!("{}Props", ident, span = ident.span());

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let (props_fields, props_binding, construction) = match &component.fields {
        Fields::Named(fields) => {
            let props_fields = fields.named.iter().map(|field| {
                let ident = field
                    .ident
                    .as_ref()
                    .expect("named component field must have an identifier");
                let ty = &field.ty;
                let attrs = conditional_attributes(field);

                quote! {
                    #( #attrs )*
                    pub #ident: #ty,
                }
            });

            let assignments = fields.named.iter().map(|field| {
                let ident = field
                    .ident
                    .as_ref()
                    .expect("named component field must have an identifier");
                let attrs = conditional_attributes(field);

                quote! {
                    #( #attrs )*
                    #ident: props.#ident,
                }
            });

            (
                quote! { #( #props_fields )* },
                quote! { props },
                quote! { Self { #( #assignments )* } },
            )
        }
        Fields::Unit => (TokenStream::new(), quote! { _props }, quote! { Self }),
        Fields::Unnamed(_) => {
            return Err(syn::Error::new_spanned(
                &component.fields,
                "`#[component] supports named-field structs and unit structs only`",
            ));
        }
    };

    Ok(quote! {
        #component

        #[doc(hidden)]
        #visibility struct #props_ident #generics {
            #props_fields
        }

        impl #impl_generics ::core::convert::From<#props_ident #ty_generics> for #ident #ty_generics
        #where_clause
        {
            fn from(#props_binding: #props_ident #ty_generics) -> Self {
                #construction
            }
        }
    })
}

fn conditional_attributes(field: &syn::Field) -> impl Iterator<Item = &Attribute> {
    field.attrs.iter().filter(|attribute| {
        attribute.path().is_ident("cfg") || attribute.path().is_ident("cfg_attr")
    })
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::{Fields, Item};

    use super::expand_component;

    #[test]
    fn expands_named_component() {
        let output = expand_component(quote! {
            pub(crate) struct Row<'a> {
                label: &'a str,
                selected: bool,
            }
        })
        .expect("component should expand");

        let file: syn::File = syn::parse2(output).expect("expanded component should be valid Rust");

        assert_eq!(file.items.len(), 3);

        let Item::Struct(props) = &file.items[1] else {
            panic!("second generated item should be the props struct");
        };

        assert_eq!(props.ident.to_string(), "RowProps");

        let Fields::Named(fields) = &props.fields else {
            panic!("props should use named fields");
        };

        assert_eq!(fields.named.len(), 2);

        for field in &fields.named {
            assert!(matches!(&field.vis, syn::Visibility::Public(_)));
        }
    }

    #[test]
    fn expands_unit_component() {
        let output = expand_component(quote! {
            pub(crate) struct LoadingIndicator;
        })
        .expect("unit component should expand");

        let file: syn::File = syn::parse2(output).expect("expanded component should be valid Rust");

        assert_eq!(file.items.len(), 3);

        let Item::Struct(props) = &file.items[1] else {
            panic!("second generated item should be the props struct");
        };

        assert_eq!(props.ident.to_string(), "LoadingIndicatorProps");

        let Fields::Named(fields) = &props.fields else {
            panic!("unit component props should be an empty named-field struct");
        };

        assert!(fields.named.is_empty());
    }

    #[test]
    fn preserves_component_generics() {
        let output = expand_component(quote! {
            pub struct Label<'a, T>
            where
                T: Copy,
            {
                text: &'a str,
                value: T,
            }
        })
        .expect("generic component should expand");

        syn::parse2::<syn::File>(output).expect("expanded generic component should be valid Rust");
    }

    #[test]
    fn rejects_tuple_component() {
        let error = expand_component(quote! {
            struct Row(&'static str);
        })
        .expect_err("tuple component should be rejected");

        assert!(
            error
                .to_string()
                .contains("supports named-field structs and unit structs")
        );
    }
}
