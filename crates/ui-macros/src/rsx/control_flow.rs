use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use rstml::{
    node::CustomNode,
    recoverable::{ParseRecoverable, RecoverableContext},
};
use syn::{Expr, Token, braced, parse::ParseStream, spanned::Spanned};

use super::{RsxNode, element::expand_child};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    OpenIf,
    ElseIf,
    Else,
    CloseIf,
}

#[derive(Clone, Debug)]
pub struct ConditionalBranch {
    pub condition: Expr,
    pub body: Vec<RsxNode>,
}

#[derive(Clone, Debug)]
pub struct IfBlock {
    pub condition: Expr,
    pub then_branch: Vec<RsxNode>,
    pub else_ifs: Vec<ConditionalBranch>,
    pub else_branch: Option<Vec<RsxNode>>,
}

#[derive(Clone, Debug)]
pub enum ControlFlow {
    If(IfBlock),
}

impl CustomNode for ControlFlow {
    fn peek_element(input: ParseStream) -> bool {
        inspect_marker(input) == Some(Marker::OpenIf)
    }
}

impl ParseRecoverable for ControlFlow {
    fn parse_recoverable(parser: &mut RecoverableContext, input: ParseStream) -> Option<Self> {
        let condition = parse_if_marker(parser, input)?;
        let then_branch = parse_branch(parser, input)?;

        let mut else_ifs = Vec::new();

        while inspect_marker(input) == Some(Marker::ElseIf) {
            let condition = parse_else_if_marker(parser, input)?;
            let body = parse_branch(parser, input)?;

            else_ifs.push(ConditionalBranch { condition, body });
        }

        let else_branch = if inspect_marker(input) == Some(Marker::Else) {
            parse_else_marker(parser, input)?;
            Some(parse_branch(parser, input)?)
        } else {
            parser.push_diagnostic(syn::Error::new(
                condition.span(),
                "`{#if}` requires a `{:else}` branch",
            ));
            None
        };

        if inspect_marker(input) == Some(Marker::CloseIf) {
            parse_close_if_marker(parser, input)?;
        } else {
            parser.push_diagnostic(syn::Error::new(
                condition.span(),
                "unclosed `{#if}` block; expected `{/if}`",
            ));
        }

        Some(Self::If(IfBlock {
            condition,
            then_branch,
            else_ifs,
            else_branch,
        }))
    }
}

impl ToTokens for ControlFlow {
    fn to_tokens(&self, _: &mut TokenStream) {
        // control flow nodes are consumed by the code generator, they are never emitted
        // back as source tokens
    }
}

pub(super) fn expand_control_flow(control_flow: &ControlFlow) -> syn::Result<TokenStream> {
    match control_flow {
        ControlFlow::If(block) => expand_if(block),
    }
}

fn expand_if(block: &IfBlock) -> syn::Result<TokenStream> {
    let else_branch = block.else_branch.as_ref().ok_or_else(|| {
        syn::Error::new(
            block.condition.span(),
            "`{#if}` requires an `{:else} branch",
        )
    })?;

    let mut fallback = expand_branch(else_branch, block.condition.span(), "`{:else}`")?;

    for branch in block.else_ifs.iter().rev() {
        let condition = &branch.condition;
        let body = expand_branch(&branch.body, condition.span(), "`{:else if ...}`")?;

        fallback = quote! {
            if #condition {
                ::inkpaper_ui::Either::Left(#body)
            } else {
                ::inkpaper_ui::Either::Right(#fallback)
            }
        };
    }

    let condition = &block.condition;
    let then_branch = expand_branch(&block.then_branch, condition.span(), "`{#if ...}`")?;

    Ok(quote! {
        if #condition {
            ::inkpaper_ui::Either::Left(#then_branch)
        } else {
            ::inkpaper_ui::Either::Right(#fallback)
        }
    })
}

fn expand_branch(
    body: &[RsxNode],
    span: proc_macro2::Span,
    branch_name: &str,
) -> syn::Result<TokenStream> {
    match body {
        [] => Err(syn::Error::new(
            span,
            format!("{branch_name} requires a renderable child"),
        )),
        [child] => expand_child(child),
        [_, second, ..] => Err(syn::Error::new_spanned(
            second,
            format!("{branch_name} accepts exactly one renderable child"),
        )),
    }
}

fn parse_branch(parser: &mut RecoverableContext, input: ParseStream) -> Option<Vec<RsxNode>> {
    let mut nodes = Vec::new();

    while !input.is_empty() {
        if let Some(Marker::ElseIf | Marker::Else | Marker::CloseIf) = inspect_marker(input) {
            break;
        }

        nodes.push(parser.parse_recoverable(input)?)
    }

    Some(nodes)
}

fn inspect_marker(input: ParseStream) -> Option<Marker> {
    let fork = input.fork();
    inspect_marker_inner(&fork).ok().flatten()
}

fn inspect_marker_inner(input: ParseStream) -> syn::Result<Option<Marker>> {
    if !input.peek(syn::token::Brace) {
        return Ok(None);
    }

    let content;
    braced!(content in input);

    if content.peek(Token![#]) {
        content.parse::<Token![#]>()?;

        if content.peek(Token![if]) {
            return Ok(Some(Marker::OpenIf));
        }

        return Ok(None);
    }

    if content.peek(Token![:]) {
        content.parse::<Token![:]>()?;

        if !content.peek(Token![else]) {
            return Ok(None);
        }

        content.parse::<Token![else]>()?;

        if content.peek(Token![if]) {
            return Ok(Some(Marker::ElseIf));
        }

        if content.is_empty() {
            return Ok(Some(Marker::Else));
        }

        return Ok(None);
    }

    if content.peek(Token![/]) {
        content.parse::<Token![/]>()?;

        if content.peek(Token![if]) {
            return Ok(Some(Marker::CloseIf));
        }
    }

    Ok(None)
}

fn parse_if_marker(parser: &mut RecoverableContext, input: ParseStream) -> Option<Expr> {
    parser.parse_mixed_fn(input, |_, input| {
        let content;
        braced!(content in input);

        content.parse::<Token![#]>()?;
        content.parse::<Token![if]>()?;

        let condition = content.parse::<Expr>()?;

        if !content.is_empty() {
            return Err(content.error("unexpected token after `{#if ...}` condition"));
        }

        Ok(condition)
    })
}

fn parse_else_if_marker(parser: &mut RecoverableContext, input: ParseStream) -> Option<Expr> {
    parser.parse_mixed_fn(input, |_, input| {
        let content;
        braced!(content in input);

        content.parse::<Token![:]>()?;
        content.parse::<Token![else]>()?;
        content.parse::<Token![if]>()?;

        let condition = content.parse::<Expr>()?;

        if !content.is_empty() {
            return Err(content.error("unexpected tokens after `{:else if ...}` condition"));
        }

        Ok(condition)
    })
}

fn parse_else_marker(parser: &mut RecoverableContext, input: ParseStream) -> Option<()> {
    parser.parse_mixed_fn(input, |_, input| {
        let content;
        braced!(content in input);

        content.parse::<Token![:]>()?;
        content.parse::<Token![else]>()?;

        if !content.is_empty() {
            return Err(content.error("unexpected tokens in `{:else}`"));
        }

        Ok(())
    })
}

fn parse_close_if_marker(parser: &mut RecoverableContext, input: ParseStream) -> Option<()> {
    parser.parse_mixed_fn(input, |_, input| {
        let content;
        braced!(content in input);

        content.parse::<Token![/]>()?;
        content.parse::<Token![if]>()?;

        if !content.is_empty() {
            return Err(content.error("unexpected tokens in `{/if}`"));
        }

        Ok(())
    })
}
