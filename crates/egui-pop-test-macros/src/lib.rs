//! Attributes for the egui-pop test kit, re-exported by `egui-pop-test`.
//!
//! [`macro@critical`] marks a test that runs on every change: see
//! `docs/testing.md` in the repository.

use proc_macro::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};

/// Marks a test as critical: it guards a rule that any change can break,
/// so a selective run (only the crates a change touches) still runs it.
///
/// The mark changes nothing in how the test runs. It renames the
/// function with a prefix the runners select by name: `critical_` for a
/// test that needs no GPU, `critical_gpu_` with `#[critical(gpu)]` for
/// one that renders. Put it above `#[test]`.
///
/// ```
/// #[egui_pop_test_macros::critical]
/// fn guards_a_rule() {}
///
/// // The function now answers to its marked name.
/// critical_guards_a_rule();
/// ```
///
/// With nextest, `-E 'test(/(^|::)critical_/)'` (or the `critical`
/// profile of the repository) selects every critical test; with plain
/// `cargo test`, the filter `critical_` does.
#[proc_macro_attribute]
pub fn critical(attr: TokenStream, item: TokenStream) -> TokenStream {
    let prefix = match prefix(attr) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let mut out = Vec::new();
    let mut after_fn = false;
    let mut renamed = false;
    for tt in item {
        match tt {
            TokenTree::Ident(ref i) if after_fn && !renamed => {
                let name = format!("{prefix}{i}");
                out.push(TokenTree::Ident(Ident::new(&name, i.span())));
                after_fn = false;
                renamed = true;
            }
            TokenTree::Ident(ref i) if !renamed && i.to_string() == "fn" => {
                after_fn = true;
                out.push(tt);
            }
            other => out.push(other),
        }
    }
    if !renamed {
        return error(
            Span::call_site(),
            "`critical` marks a test function: put it on a `fn`",
        );
    }
    out.into_iter().collect()
}

/// The prefix for the attribute's arguments: none, or `gpu`.
fn prefix(attr: TokenStream) -> Result<&'static str, TokenStream> {
    let args: Vec<TokenTree> = attr.into_iter().collect();
    match args.as_slice() {
        [] => Ok("critical_"),
        [TokenTree::Ident(i)] if i.to_string() == "gpu" => Ok("critical_gpu_"),
        [first, ..] => Err(error(
            first.span(),
            "`critical` takes no argument, or `gpu` for a test that renders",
        )),
    }
}

/// `compile_error!("message")` at this place.
fn error(span: Span, message: &str) -> TokenStream {
    let mut lit: TokenStream = format!("{message:?}").parse().expect("a string literal");
    lit = lit
        .into_iter()
        .map(|mut t| {
            t.set_span(span);
            t
        })
        .collect();
    let mut bang = Punct::new('!', Spacing::Alone);
    bang.set_span(span);
    let mut semi = Punct::new(';', Spacing::Alone);
    semi.set_span(span);
    let mut args = Group::new(Delimiter::Parenthesis, lit);
    args.set_span(span);
    [
        TokenTree::Ident(Ident::new("compile_error", span)),
        TokenTree::Punct(bang),
        TokenTree::Group(args),
        TokenTree::Punct(semi),
    ]
    .into_iter()
    .collect()
}
