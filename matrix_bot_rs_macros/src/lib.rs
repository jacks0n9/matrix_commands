use darling::ast::NestedMeta;
use darling::{Error, FromMeta};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, FnArg, GenericArgument, Ident, ItemFn, LitStr, Pat, PathArguments, Type};
#[proc_macro_attribute]
pub fn bot_command(args: TokenStream, code: TokenStream) -> TokenStream {
    let attr_args = match NestedMeta::parse_meta_list(args.into()) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(Error::from(e).write_errors());
        }
    };
    let cloned = code.clone();
    let args = match MacroArgs::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };
    let mut parsed = parse_macro_input!(cloned as ItemFn);
    let mut fn_args = parsed.sig.inputs.iter();
    fn_args.next();
    let mut conversions = Vec::new();
    let mut handler_arg_names = Vec::new();
    let mut arg_hints = Vec::new();
    for arg in fn_args {
        let pat_type = match arg {
            FnArg::Typed(pat) => pat,
            _ => panic!("arg cannot be self"),
        };
        let mut actual_type = pat_type.ty.clone();
        let is_optional: bool = match pat_type.ty.as_ref() {
            Type::Path(path) => match path.path.segments.first() {
                Some(segment) => {
                    let is_optional = segment.ident.to_string() == "Option";
                    if is_optional {
                        match &segment.arguments{
                            PathArguments::AngleBracketed(bracketed)=>{
                                for arg in &bracketed.args{
                                    match arg{
                                        GenericArgument::Type(ty)=>{
                                            actual_type=Box::new(ty.clone());
                                            break
                                        },
                                        _=>{}
                                    }
                                }
                            },
                            _=>{}
                        }
                    }
                    is_optional
                }
                None => false,
            },
            _ => false,
        };
        println!("{is_optional}");
        let as_metas: Vec<_> = pat_type
            .attrs
            .clone()
            .into_iter()
            .map(|attr| darling::ast::NestedMeta::Meta(attr.meta))
            .collect();
        let arg_parameters = ArgParameters::from_list(&as_metas).unwrap();
        let ident = match pat_type.pat.as_ref() {
            Pat::Ident(ident) => ident,
            _ => continue,
        };
        handler_arg_names.push(ident);
        let name = &ident.ident;
        let hint_name = arg_parameters.name.unwrap_or(name.to_string());
        let description = arg_parameters.description;
        arg_hints.push(quote! {
            matrix_bot_rs::CommandArgHint{
                name: #hint_name,
                description: #description
            }
        });

        let conversion = quote! {
            let (#name,input)=<#actual_type as matrix_bot_rs::TryFromStr>::try_from_str(&input).map_err(|e|CommandError::ArgParseError(e))?;
        };
        conversions.push(conversion);
    }

    let og_ident = parsed.sig.ident.clone();
    parsed.sig.ident = Ident::new("inner_fn", Span::call_site());
    let name = args.name.unwrap_or(og_ident.to_string());
    let aliases = args.aliases;
    let generated: TokenStream = quote! {
        async fn #og_ident<'a>()->matrix_bot_rs::Command<'a>{
            use matrix_bot_rs::CallingContext;
            async fn handler(ctx: CallingContext<'_>,input: String)->Result<(),CommandError>{
                #(#conversions)*
                inner_fn(ctx,#(#handler_arg_names),*).await?;
                #parsed
                Ok(())
            }
            fn handler_pinned(ctx: CallingContext,args: String)->matrix_bot_rs::AsyncHandlerReturn{
                Box::pin(handler(ctx,args))
            }
            matrix_bot_rs::Command{
                name: #name,
                aliases: &[#(#aliases),*],
                power_level_required: 0,
                arg_hints: &[#(#arg_hints),*],
                handler:handler_pinned
            }
        }

    }
    .into();
    generated
}
#[derive(FromMeta, Default)]
#[darling(default)]

struct MacroArgs {
    name: Option<String>,
    aliases: Vec<LitStr>,
}
#[derive(FromMeta, Default)]
#[darling(default)]
struct ArgParameters {
    name: Option<String>,
    description: String,
}
