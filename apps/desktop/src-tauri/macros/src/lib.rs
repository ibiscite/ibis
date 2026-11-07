//! Each desktop command's argument and result types, recorded from its own
//! signature so that the generated TypeScript cannot drift from the Rust.
//!
//! `#[command]` goes under `#[tauri::command]` and adds a hidden, test-only
//! `__schema_NAME` function describing the command through `schemars`;
//! `commands![…]` lists those functions for the same paths that
//! `tauri::generate_handler!` receives.
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Expr, ExprLit, FnArg, GenericArgument, ItemFn, Lit, Meta, Pat, Path, PathArguments, ReturnType, Token, Type,
    parse_macro_input, punctuated::Punctuated,
};

/// Arguments Tauri supplies itself; the web view never sends them.
const INJECTED: [&str; 5] = ["AppHandle", "State", "Window", "WebviewWindow", "Webview"];

#[proc_macro_attribute]
pub fn command(_: TokenStream, item: TokenStream) -> TokenStream {
    let function = parse_macro_input!(item as ItemFn);
    let name = &function.sig.ident;
    let schema = format_ident!("__schema_{}", name);
    let visibility = &function.vis;
    let docs = docs(&function);
    let mut args = Vec::new();
    for input in &function.sig.inputs {
        let FnArg::Typed(typed) = input else {
            return error(input, "a command is a free function");
        };
        if last_segment(&typed.ty).is_some_and(|segment| INJECTED.contains(&segment.as_str())) {
            continue;
        }
        let Pat::Ident(pattern) = typed.pat.as_ref() else {
            return error(&typed.pat, "a command argument needs a plain name");
        };
        let wire = camel_case(&pattern.ident.to_string());
        let ty = &typed.ty;
        args.push(quote! { (#wire, schemas.input::<#ty>()) });
    }
    // The web view receives the success value; an error is thrown.
    let (returns, error) = match &function.sig.output {
        ReturnType::Default => (quote! { None }, quote! { None }),
        ReturnType::Type(_, ty) => match result_types(ty) {
            Some((ok, error)) => (
                quote! { Some(schemas.output::<#ok>()) },
                quote! { Some(schemas.output::<#error>()) },
            ),
            None => (quote! { Some(schemas.output::<#ty>()) }, quote! { None }),
        },
    };
    let wire_name = name.to_string();
    quote! {
        #function
        #[cfg(test)]
        #[doc(hidden)]
        #visibility fn #schema(schemas: &mut crate::bindings::Schemas) -> crate::bindings::Command {
            crate::bindings::Command {
                name: #wire_name,
                docs: #docs,
                args: vec![#(#args),*],
                returns: #returns,
                error: #error,
            }
        }
    }
    .into()
}

/// The schema functions of these commands, in order.
#[proc_macro]
pub fn commands(input: TokenStream) -> TokenStream {
    let paths = parse_macro_input!(input with Punctuated::<Path, Token![,]>::parse_terminated);
    let schemas = paths.iter().map(|path| {
        let mut path = path.clone();
        if let Some(last) = path.segments.last_mut() {
            last.ident = format_ident!("__schema_{}", last.ident);
            last.arguments = PathArguments::None;
        }
        quote! { #path as fn(&mut crate::bindings::Schemas) -> crate::bindings::Command }
    });
    quote! { vec![#(#schemas),*] }.into()
}

fn docs(function: &ItemFn) -> String {
    function
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("doc"))
        .filter_map(|attribute| match &attribute.meta {
            Meta::NameValue(pair) => match &pair.value {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(text), ..
                }) => Some(text.value()),
                _ => None,
            },
            _ => None,
        })
        .map(|line| line.strip_prefix(' ').map(str::to_owned).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn last_segment(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path.path.segments.last().map(|segment| segment.ident.to_string()),
        Type::Reference(reference) => last_segment(&reference.elem),
        _ => None,
    }
}

/// `T` and `E` in `Result<T, E>`.
fn result_types(ty: &Type) -> Option<(&Type, &Type)> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if segment.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let mut types = arguments.args.iter().filter_map(|argument| match argument {
        GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    Some((types.next()?, types.next()?))
}

/// Tauri's default argument names in the web view.
fn camel_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = false;
    for character in name.chars() {
        if character == '_' {
            upper = true;
        } else if upper {
            out.extend(character.to_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

fn error(spanned: impl quote::ToTokens, message: &str) -> TokenStream {
    syn::Error::new_spanned(spanned, message).to_compile_error().into()
}
