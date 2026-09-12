use proc_macro2::Span;
use rstml::node::{KeyedAttribute, NodeAttribute};
use syn::{Expr, Ident, Lit, Stmt};

use crate::rsx::RsxElement;

pub(super) type ClassAttribute = Option<(String, Span)>;

pub(super) struct ImageAttributes<'a> {
    pub source: &'a Expr,
    pub class: ClassAttribute,
}

pub(super) struct ComponentProp<'a> {
    pub name: Ident,
    pub value: &'a Expr,
}

pub(super) fn parse_class_attribute(element: &RsxElement) -> syn::Result<ClassAttribute> {
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

        class = Some(parse_class_literal(attribute)?);
    }

    Ok(class)
}

pub(super) fn parse_image_attributes<'a>(
    element: &'a RsxElement,
) -> syn::Result<ImageAttributes<'a>> {
    let mut source = None;
    let mut class = None;

    for attribute in element.attributes() {
        let NodeAttribute::Attribute(attribute) = attribute else {
            return Err(syn::Error::new_spanned(
                attribute,
                "dynamic <image> attributes are not supported",
            ));
        };

        let name = attribute.key.to_string();

        match name.as_str() {
            "source" => {
                if source.is_some() {
                    return Err(syn::Error::new_spanned(
                        attribute,
                        "duplicate `source` attribute",
                    ));
                }

                let Some(value) = attribute.value() else {
                    return Err(syn::Error::new_spanned(
                        attribute,
                        "`source` requires a Rust expression",
                    ));
                };

                source = Some(unbrace_expr(value));
            }

            "class" => {
                if class.is_some() {
                    return Err(syn::Error::new_spanned(
                        attribute,
                        "duplicate `class` attribute",
                    ));
                }

                class = Some(parse_class_literal(attribute)?);
            }

            _ => {
                return Err(syn::Error::new_spanned(
                    attribute,
                    format!("unsupported <image> attribute `{name}`"),
                ));
            }
        }
    }

    let Some(source) = source else {
        return Err(syn::Error::new_spanned(
            element,
            "<image> requires a `source` attribute",
        ));
    };

    Ok(ImageAttributes { source, class })
}

pub(super) fn parse_component_props<'a>(
    element: &'a RsxElement,
) -> syn::Result<Vec<ComponentProp<'a>>> {
    let mut props = Vec::new();

    for attribute in element.attributes() {
        let NodeAttribute::Attribute(attribute) = attribute else {
            return Err(syn::Error::new_spanned(
                attribute,
                "dynamic component attributes are not supported",
            ));
        };

        let name = attribute.key.to_string();

        let ident = syn::parse_str::<Ident>(&name).map_err(|error| {
            syn::Error::new_spanned(
                attribute,
                format!("invalid component prop name `{name}`: {error}"),
            )
        })?;

        if props
            .iter()
            .any(|prop: &ComponentProp<'_>| prop.name == ident)
        {
            return Err(syn::Error::new_spanned(
                attribute,
                format!("duplicate component prop `{name}`"),
            ));
        }

        let Some(value) = attribute.value() else {
            return Err(syn::Error::new_spanned(
                attribute,
                format!("component prop `{name}` requires a Rust expression"),
            ));
        };

        props.push(ComponentProp {
            name: ident,
            value: unbrace_expr(value),
        });
    }

    Ok(props)
}

fn parse_class_literal(attribute: &KeyedAttribute) -> syn::Result<(String, Span)> {
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

    Ok((value.value(), value.span()))
}

pub(super) fn unbrace_expr(expression: &Expr) -> &Expr {
    let Expr::Block(block) = expression else {
        return expression;
    };

    if !block.attrs.is_empty() || block.label.is_some() {
        return expression;
    }

    let [Stmt::Expr(inner, None)] = block.block.stmts.as_slice() else {
        return expression;
    };

    unbrace_expr(inner)
}
