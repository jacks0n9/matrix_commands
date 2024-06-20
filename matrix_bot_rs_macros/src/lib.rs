use proc_macro::TokenStream;
use darling::{Error, FromMeta};
use quote::quote;
use darling::ast::NestedMeta;
use syn::{parse_macro_input,ItemFn,LitStr,FnArg,Pat,Ident};
use proc_macro2::Span;
#[proc_macro_attribute]
pub fn bot_command(args: TokenStream,code: TokenStream)->TokenStream{
    let attr_args = match NestedMeta::parse_meta_list(args.into()) {
        Ok(v) => v,
        Err(e) => { return TokenStream::from(Error::from(e).write_errors()); }
    };
    let cloned=code.clone();
    let args=match MacroArgs::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => { return TokenStream::from(e.write_errors()); }
    };
    let mut parsed=parse_macro_input!(cloned as ItemFn);
    let mut fn_args=parsed.sig.inputs.iter();
    fn_args.next();
    let mut conversions=Vec::new();
    let mut handler_arg_names=Vec::new();
    let mut arg_hints=Vec::new();
    for arg in fn_args{
        let pat_type=match arg{
            FnArg::Typed(pat)=>pat,
            _=>panic!("arg cannot be self")
        };
        let as_metas:Vec<_>=pat_type.attrs.clone().into_iter().map(|attr|darling::ast::NestedMeta::Meta(attr.meta)).collect();
        let arg_parameters=ArgParameters::from_list(&as_metas).unwrap();
        let ident=match pat_type.pat.as_ref(){
            Pat::Ident(ident)=>ident,
            _=>continue
        };
        handler_arg_names.push(ident);
        let name=&ident.ident;
        let hint_name=arg_parameters.name.unwrap_or(name.to_string());
        let description=arg_parameters.description;
        arg_hints.push(quote!{
            matrix_bot_rs::CommandArgHint{
                name: #hint_name,
                description: #description
            }
        });
        
        let actual_type=&pat_type.ty;

        let conversion=quote!{
            let (#name,input)=<#actual_type as TryFromStr>::try_from_string(&input).map_err(|e|CommandError::ArgParseError(e))?;
        };
        conversions.push(conversion);
    }
    
    let og_ident=parsed.sig.ident.clone();
    parsed.sig.ident=Ident::new("inner_fn", Span::call_site());
    let name=args.name.unwrap_or(og_ident.to_string());
    let aliases=args.aliases;
    let generated: TokenStream=quote!{
        fn #og_ident<'a>()->matrix_bot_rs::Command<'a>{
            use matrix_bot_rs::CallingContext;
            async fn handler(ctx: CallingContext<'_>,input: String)->Result<(),CommandError>{
                #(#conversions)*
                inner_fn(ctx,#(#handler_arg_names),*).await?;
                #parsed
                Ok(())
            }
            matrix_bot_rs::Command{
                name: #name,
                aliases: &[#(#aliases),*],
                power_level_required: 0,
                arg_hints: &[#(#arg_hints),*],
                handler:|a,b|Box::pin(handler(a,b))
            }
        }
        
    }.into();
    generated

}
#[derive(FromMeta,Default)]
#[darling(default)]

struct MacroArgs{
    name: Option<String>,
    aliases: Vec<LitStr>
}
#[derive(FromMeta,Default)]
#[darling(default)]
struct ArgParameters{
    name: Option<String>,
    description: String,
}