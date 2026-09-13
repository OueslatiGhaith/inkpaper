use proc_macro2::Span;
use rstml::node::{KeyedAttribute, NodeAttribute};
use syn::{Expr, Ident, Lit, Stmt};

use crate::rsx::RsxElement;

pub(super) type ClassAttribute = Option<(String, Span)>;

pub(super) enum EventAttributeKind {
    Activate,
    Typed(Ident),
}

pub(super) struct EventAttribute<'a> {
    pub name: String,
    pub kind: EventAttributeKind,
    pub listener: &'a Expr,
}

pub(super) struct IntrinsicAttributes<'a> {
    pub class: ClassAttribute,
    pub id: Option<&'a Expr>,
    pub focusable: bool,
    pub events: Vec<EventAttribute<'a>>,
}

impl IntrinsicAttributes<'_> {
    fn new() -> Self {
        Self {
            class: None,
            id: None,
            focusable: false,
            events: Vec::new(),
        }
    }
}

pub(super) struct ImageAttributes<'a> {
    pub source: &'a Expr,
    pub intrinsic: IntrinsicAttributes<'a>,
}

pub(super) struct ComponentProp<'a> {
    pub name: Ident,
    pub value: &'a Expr,
}

fn parse_event_attribute<'a>(
    attribute: &'a KeyedAttribute,
    attributes: &mut IntrinsicAttributes<'a>,
) -> syn::Result<bool> {
    let name = attribute.key.to_string();

    let Some(event_name) = name.strip_prefix("on:") else {
        return Ok(false);
    };

    if event_name.is_empty() {
        return Err(syn::Error::new_spanned(
            attribute,
            "`on:` requires an event name",
        ));
    }

    if attributes
        .events
        .iter()
        .any(|event| event.name == event_name)
    {
        return Err(syn::Error::new_spanned(
            attribute,
            format!("duplicate `on:{event_name}` attribute"),
        ));
    }

    let Some(value) = attribute.value() else {
        return Err(syn::Error::new_spanned(
            attribute,
            format!("`on:{event_name}` requires a listener"),
        ));
    };

    let kind = match event_name {
        "activate" => EventAttributeKind::Activate,
        _ => {
            let Some(first) = event_name.chars().next() else {
                unreachable!()
            };

            if !first.is_uppercase() {
                return Err(syn::Error::new_spanned(
                    attribute,
                    format!("unknown built-in event `{event_name}`"),
                ));
            }

            let event_type = syn::parse_str::<Ident>(event_name).map_err(|_| {
                syn::Error::new_spanned(
                    attribute,
                    "custom event names must be a single Rust type identifier",
                )
            })?;

            EventAttributeKind::Typed(event_type)
        }
    };

    attributes.events.push(EventAttribute {
        name: event_name.to_owned(),
        kind,
        listener: unbrace_expr(value),
    });

    Ok(true)
}

pub(super) fn parse_intrinsic_attributes(element: &RsxElement) -> syn::Result<IntrinsicAttributes> {
    let mut attributes = IntrinsicAttributes::new();

    for attribute in element.attributes() {
        let NodeAttribute::Attribute(attribute) = attribute else {
            return Err(syn::Error::new_spanned(
                attribute,
                "dynamic rsx! attributes are not supported yet",
            ));
        };

        if parse_intrinsic_attribute(attribute, &mut attributes)? {
            continue;
        }

        let name = attribute.key.to_string();

        return Err(syn::Error::new_spanned(
            attribute,
            format!("unsupported attribute `{name}`"),
        ));
    }

    Ok(attributes)
}

fn parse_intrinsic_attribute<'a>(
    attribute: &'a KeyedAttribute,
    attributes: &mut IntrinsicAttributes<'a>,
) -> syn::Result<bool> {
    if parse_event_attribute(attribute, attributes)? {
        return Ok(true);
    }

    let name = attribute.key.to_string();

    match name.as_str() {
        "class" => {
            if attributes.class.is_some() {
                return Err(syn::Error::new_spanned(
                    attribute,
                    "duplicate `class` attribute",
                ));
            }

            attributes.class = Some(parse_class_literal(attribute)?);
            Ok(true)
        }
        "id" => {
            if attributes.id.is_some() {
                return Err(syn::Error::new_spanned(
                    attribute,
                    "duplicate `id` attribute",
                ));
            }

            let Some(value) = attribute.value() else {
                return Err(syn::Error::new_spanned(attribute, "`id` requires a value"));
            };

            attributes.id = Some(unbrace_expr(value));
            Ok(true)
        }
        "focusable" => {
            if attributes.focusable {
                return Err(syn::Error::new_spanned(
                    attribute,
                    "duplicate `focusable` attribute",
                ));
            }

            if attribute.value().is_some() {
                return Err(syn::Error::new_spanned(
                    attribute,
                    "`focusable` is a boolean attribute and must not have a value",
                ));
            }

            attributes.focusable = true;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub(super) fn parse_image_attributes<'a>(
    element: &'a RsxElement,
) -> syn::Result<ImageAttributes<'a>> {
    let mut source = None;
    let mut intrinsic = IntrinsicAttributes::new();

    for attribute in element.attributes() {
        let NodeAttribute::Attribute(attribute) = attribute else {
            return Err(syn::Error::new_spanned(
                attribute,
                "dynamic <image> attributes are not supported",
            ));
        };

        if parse_intrinsic_attribute(attribute, &mut intrinsic)? {
            continue;
        }

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

    Ok(ImageAttributes { source, intrinsic })
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
