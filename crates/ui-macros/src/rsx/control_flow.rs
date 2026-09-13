use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use quote::{ToTokens, quote};
use rstml::{
    node::CustomNode,
    recoverable::{ParseRecoverable, RecoverableContext},
};
use syn::{
    Expr, Ident, Pat, Token, braced,
    parse::{ParseStream, Parser as _},
    spanned::Spanned,
};

use crate::rsx::element::expand_keyed_child;

use super::{
    RsxNode,
    element::{
        expand_child, expand_children_group, expand_children_group_into, expand_children_into,
    },
};

mod kw {
    syn::custom_keyword!(each);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    OpenIf,
    OpenEach,
    ElseIf,
    Else,
    CloseIf,
    CloseEach,
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
pub struct EachBlock {
    pub iterable: Expr,
    pub pattern: Pat,
    pub index: Option<Ident>,
    pub key: Option<Expr>,
    pub body: Vec<RsxNode>,
    pub else_branch: Option<Vec<RsxNode>>,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ControlFlow {
    If(IfBlock),
    Each(EachBlock),
}

impl CustomNode for ControlFlow {
    fn peek_element(input: ParseStream) -> bool {
        matches!(
            inspect_marker(input),
            Some(Marker::OpenIf | Marker::OpenEach)
        )
    }
}

impl ParseRecoverable for ControlFlow {
    fn parse_recoverable(parser: &mut RecoverableContext, input: ParseStream) -> Option<Self> {
        match inspect_marker(input) {
            Some(Marker::OpenIf) => parse_if_block(parser, input).map(Self::If),
            Some(Marker::OpenEach) => parse_each_block(parser, input).map(Self::Each),
            _ => {
                parser.push_diagnostic(syn::Error::new(
                    input.span(),
                    "expected an rsx! control-flow block",
                ));

                None
            }
        }
    }
}

impl ToTokens for ControlFlow {
    fn to_tokens(&self, _: &mut TokenStream) {
        // control-flow nodes are consumed by the code generator
    }
}

pub(super) fn expand_control_flow(control_flow: &ControlFlow) -> syn::Result<TokenStream> {
    match control_flow {
        ControlFlow::If(block) => expand_if(block),
        ControlFlow::Each(block) => Err(syn::Error::new(
            block.iterable.span(),
            "root `{#each}` is not supported because rsx! must produce exactly one root element",
        )),
    }
}

pub(super) fn expand_control_flow_into(
    parent: TokenStream,
    control_flow: &ControlFlow,
) -> syn::Result<TokenStream> {
    match control_flow {
        ControlFlow::If(block) => expand_if_into(parent, block),
        ControlFlow::Each(block) => expand_each_into(parent, block),
    }
}

pub(super) fn expand_control_flow_group_into(
    group: TokenStream,
    control_flow: &ControlFlow,
) -> syn::Result<TokenStream> {
    match control_flow {
        ControlFlow::If(block) => expand_if_group_into(group, block),
        ControlFlow::Each(block) => expand_each_group_into(group, block),
    }
}

fn parse_if_block(parser: &mut RecoverableContext, input: ParseStream) -> Option<IfBlock> {
    let condition = parse_if_marker(parser, input)?;
    let then_branch = parse_if_branch(parser, input)?;

    let mut else_ifs = Vec::new();

    while inspect_marker(input) == Some(Marker::ElseIf) {
        let condition = parse_else_if_marker(parser, input)?;
        let body = parse_if_branch(parser, input)?;

        else_ifs.push(ConditionalBranch { condition, body });
    }

    let else_branch = if inspect_marker(input) == Some(Marker::Else) {
        parse_else_marker(parser, input)?;

        Some(parse_if_branch(parser, input)?)
    } else {
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

    Some(IfBlock {
        condition,
        then_branch,
        else_ifs,
        else_branch,
    })
}

fn parse_each_block(parser: &mut RecoverableContext, input: ParseStream) -> Option<EachBlock> {
    let (iterable, pattern, index, key) = parse_each_marker(parser, input)?;

    let body = parse_each_branch(parser, input)?;

    let else_branch = if inspect_marker(input) == Some(Marker::Else) {
        parse_else_marker(parser, input)?;

        Some(parse_each_branch(parser, input)?)
    } else {
        None
    };

    if inspect_marker(input) == Some(Marker::CloseEach) {
        parse_close_each_marker(parser, input)?;
    } else {
        parser.push_diagnostic(syn::Error::new(
            iterable.span(),
            "unclosed `{#each}` block; expected `{/each}`",
        ));
    }

    Some(EachBlock {
        iterable,
        pattern,
        index,
        key,
        body,
        else_branch,
    })
}

fn expand_if(block: &IfBlock) -> syn::Result<TokenStream> {
    let else_branch = block.else_branch.as_ref().ok_or_else(|| {
        syn::Error::new(
            block.condition.span(),
            "root `{#if}` requires a `{:else}` branch because rsx! must always produce one root element",
        )
    })?;

    let mut fallback = expand_root_branch(else_branch, block.condition.span(), "`{:else}`")?;

    for branch in block.else_ifs.iter().rev() {
        let condition = &branch.condition;
        let body = expand_root_branch(&branch.body, condition.span(), "`{:else if ...}`")?;

        fallback = quote! {
            if #condition {
                ::inkpaper_ui::Either::Left(#body)
            } else {
                ::inkpaper_ui::Either::Right(#fallback)
            }
        };
    }

    let condition = &block.condition;
    let then_branch = expand_root_branch(&block.then_branch, condition.span(), "`{#if ...}`")?;

    Ok(quote! {
        if #condition {
            ::inkpaper_ui::Either::Left(#then_branch)
        } else {
            ::inkpaper_ui::Either::Right(#fallback)
        }
    })
}

fn expand_if_into(parent: TokenStream, block: &IfBlock) -> syn::Result<TokenStream> {
    let parent_ident = Ident::new("__inkpaper_rsx_parent", Span::mixed_site());
    let parent_ref = quote! { #parent_ident };

    let mut fallback = match &block.else_branch {
        Some(else_branch) => expand_children_into(parent_ref.clone(), else_branch)?,
        None => parent_ref.clone(),
    };

    for branch in block.else_ifs.iter().rev() {
        let condition = &branch.condition;
        let body = expand_children_into(parent_ref.clone(), &branch.body)?;

        fallback = quote! {
            if #condition {
                ::inkpaper_ui::Either::Left(#body)
            } else {
                ::inkpaper_ui::Either::Right(#fallback)
            }
        };
    }

    let condition = &block.condition;
    let then_branch = expand_children_into(parent_ref, &block.then_branch)?;

    Ok(quote! {
        {
            let #parent_ident = #parent;

            if #condition {
                ::inkpaper_ui::Either::Left(#then_branch)
            } else {
                ::inkpaper_ui::Either::Right(#fallback)
            }
        }
    })
}

fn expand_if_group_into(group: TokenStream, block: &IfBlock) -> syn::Result<TokenStream> {
    let group_ident = Ident::new("__inkpaper_rsx_children", Span::mixed_site());
    let group_ref = quote! { #group_ident };

    let mut fallback = match &block.else_branch {
        Some(else_branch) => expand_children_group_into(group_ref.clone(), else_branch)?,
        None => group_ref.clone(),
    };

    for branch in block.else_ifs.iter().rev() {
        let condition = &branch.condition;
        let body = expand_children_group_into(group_ref.clone(), &branch.body)?;

        fallback = quote! {
            if #condition {
                ::inkpaper_ui::Either::Left(#body)
            } else {
                ::inkpaper_ui::Either::Right(#fallback)
            }
        };
    }

    let condition = &block.condition;
    let then_branch = expand_children_group_into(group_ref, &block.then_branch)?;

    Ok(quote! {
        {
            let #group_ident = #group;

            if #condition {
                ::inkpaper_ui::Either::Left(#then_branch)
            } else {
                ::inkpaper_ui::Either::Right(#fallback)
            }
        }
    })
}

fn expand_each_into(parent: TokenStream, block: &EachBlock) -> syn::Result<TokenStream> {
    let children = expand_each_group_into(quote! { ::inkpaper_ui::NoChildren }, block)?;

    Ok(quote! { ::inkpaper_ui::ParentElementChildrenExt::child_sequence(#parent, #children) })
}

fn expand_each_group_into(group: TokenStream, block: &EachBlock) -> syn::Result<TokenStream> {
    let body = match &block.key {
        Some(key) => expand_keyed_each_body(&block.body, key)?,
        None => expand_children_group(&block.body)?,
    };

    let fallback = match &block.else_branch {
        Some(else_branch) => expand_children_group(else_branch)?,
        None => quote! { ::inkpaper_ui::NoChildren },
    };

    let iterable = &block.iterable;
    let pattern = &block.pattern;

    let (items, binding) = match &block.index {
        Some(index) => (
            quote! {
                ::core::iter::Iterator::enumerate(
                    ::core::iter::IntoIterator::into_iter(#iterable)
                )
            },
            quote! { (#index, #pattern) },
        ),
        None => (quote! { #iterable }, quote! { #pattern }),
    };

    Ok(quote! {
        ::inkpaper_ui::ChildrenExt::each(
            #group,
            #items,
            |#binding| {
                #body
            },
            || {
                #fallback
            },
        )
    })
}

fn expand_keyed_each_body(body: &[RsxNode], key: &Expr) -> syn::Result<TokenStream> {
    let child = match body {
        [] => {
            return Err(syn::Error::new(
                key.span(),
                "keyed `{#each}` requires one root element",
            ));
        }
        [child] => expand_keyed_child(child, key)?,
        [_, second, ..] => {
            return Err(syn::Error::new_spanned(
                second,
                "keyed `{#each}` requires exactly one root element",
            ));
        }
    };

    Ok(quote! { ::inkpaper_ui::ChildrenExt::child(::inkpaper_ui::NoChildren, #child) })
}

fn expand_root_branch(body: &[RsxNode], span: Span, branch_name: &str) -> syn::Result<TokenStream> {
    match body {
        [] => Err(syn::Error::new(
            span,
            format!("{branch_name} requires a renderable root element"),
        )),
        [child] => expand_child(child),
        [_, second, ..] => Err(syn::Error::new_spanned(
            second,
            format!(
                "{branch_name} accepts exactly one root element when the conditional itself is the rsx! root"
            ),
        )),
    }
}

fn parse_if_branch(parser: &mut RecoverableContext, input: ParseStream) -> Option<Vec<RsxNode>> {
    let mut nodes = Vec::new();

    while !input.is_empty() {
        if starts_element_close(input)
            || matches!(
                inspect_marker(input),
                Some(Marker::ElseIf | Marker::Else | Marker::CloseIf | Marker::CloseEach)
            )
        {
            break;
        }

        nodes.push(parser.parse_recoverable(input)?);
    }

    Some(nodes)
}

fn parse_each_branch(parser: &mut RecoverableContext, input: ParseStream) -> Option<Vec<RsxNode>> {
    let mut nodes = Vec::new();

    while !input.is_empty() {
        if starts_element_close(input)
            || matches!(
                inspect_marker(input),
                Some(Marker::Else | Marker::ElseIf | Marker::CloseEach | Marker::CloseIf)
            )
        {
            break;
        }

        nodes.push(parser.parse_recoverable(input)?);
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

        if content.peek(kw::each) {
            return Ok(Some(Marker::OpenEach));
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

        if content.peek(kw::each) {
            return Ok(Some(Marker::CloseEach));
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

fn parse_each_marker(
    parser: &mut RecoverableContext,
    input: ParseStream,
) -> Option<(Expr, Pat, Option<Ident>, Option<Expr>)> {
    parser.parse_mixed_fn(input, |_, input| {
        let content;
        braced!(content in input);

        content.parse::<Token![#]>()?;
        content.parse::<kw::each>()?;

        let head = content.parse::<TokenStream>()?;

        parse_each_head(head)
    })
}

fn parse_each_head(head: TokenStream) -> syn::Result<(Expr, Pat, Option<Ident>, Option<Expr>)> {
    let tokens = head.into_iter().collect::<Vec<_>>();

    let span = tokens
        .first()
        .map(TokenTree::span)
        .unwrap_or_else(Span::call_site);

    let Some(as_index) = tokens.iter().rposition(|token| {
        matches!(
            token,
            TokenTree::Ident(ident)
                if ident == "as"
        )
    }) else {
        return Err(syn::Error::new(
            span,
            "`{#each}` requires `as`, for example `{#each items as item}`",
        ));
    };

    if as_index == 0 {
        return Err(syn::Error::new(
            tokens[as_index].span(),
            "`{#each}` requires an iterable expression before `as`",
        ));
    }

    if as_index + 1 >= tokens.len() {
        return Err(syn::Error::new(
            tokens[as_index].span(),
            "`{#each}` requires a binding after `as`",
        ));
    }

    let iterable_tokens = tokens[..as_index].iter().cloned().collect::<TokenStream>();
    let raw_binding_tokens = &tokens[as_index + 1..];
    let (binding_tokens, key) = split_each_key(raw_binding_tokens)?;

    let commas = binding_tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| match token {
            TokenTree::Punct(punct) if punct.as_char() == ',' => Some(index),
            _ => None,
        })
        .collect::<Vec<_>>();

    if commas.len() > 1 {
        return Err(syn::Error::new(
            binding_tokens[commas[1]].span(),
            "`{#each}` accepts at most one index binding",
        ));
    }

    let (pattern_tokens, index_tokens) = match commas.first().copied() {
        Some(comma) => (
            binding_tokens[..comma]
                .iter()
                .cloned()
                .collect::<TokenStream>(),
            Some(
                binding_tokens[comma + 1..]
                    .iter()
                    .cloned()
                    .collect::<TokenStream>(),
            ),
        ),
        None => (
            binding_tokens.iter().cloned().collect::<TokenStream>(),
            None,
        ),
    };

    if pattern_tokens.is_empty() {
        return Err(syn::Error::new(
            tokens[as_index].span(),
            "`{#each}` requires a binding after `as`",
        ));
    }

    let iterable = syn::parse2::<Expr>(iterable_tokens)?;
    let pattern = Pat::parse_multi_with_leading_vert.parse2(pattern_tokens)?;

    let index = match index_tokens {
        Some(tokens) if tokens.is_empty() => {
            return Err(syn::Error::new(
                span,
                "`{#each}` requires an index name after `,`",
            ));
        }
        Some(tokens) => Some(syn::parse2::<Ident>(tokens).map_err(|error| {
            syn::Error::new(error.span(), "`{#each}` index must be an identifier")
        })?),
        None => None,
    };

    Ok((iterable, pattern, index, key))
}

fn split_each_key(tokens: &[TokenTree]) -> syn::Result<(Vec<TokenTree>, Option<Expr>)> {
    let Some(TokenTree::Group(group)) = tokens.last() else {
        return Ok((tokens.to_vec(), None));
    };

    if group.delimiter() != Delimiter::Parenthesis || tokens.len() == 1 {
        return Ok((tokens.to_vec(), None));
    }

    if group.stream().is_empty() {
        return Err(syn::Error::new(
            group.span(),
            "keyed `{#each}` requires an expression inside `(...)`",
        ));
    }

    let key = syn::parse2::<Expr>(group.stream()).map_err(|error| {
        syn::Error::new(
            error.span(),
            format!("invalid keyed `{{#each}}` expression: {error}"),
        )
    })?;

    Ok((tokens[..tokens.len() - 1].to_vec(), Some(key)))
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

fn parse_close_each_marker(parser: &mut RecoverableContext, input: ParseStream) -> Option<()> {
    parser.parse_mixed_fn(input, |_, input| {
        let content;
        braced!(content in input);

        content.parse::<Token![/]>()?;
        content.parse::<kw::each>()?;

        if !content.is_empty() {
            return Err(content.error("unexpected tokens in `{/each}`"));
        }

        Ok(())
    })
}

fn starts_element_close(input: ParseStream) -> bool {
    let fork = input.fork();

    if !fork.peek(Token![<]) {
        return false;
    }

    if fork.parse::<Token![<]>().is_err() {
        return false;
    }

    fork.peek(Token![/])
}
