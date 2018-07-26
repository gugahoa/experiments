#![feature(proc_macro_lib, proc_macro_diagnostic)]
#![allow(dead_code, unused_variables)]

extern crate proc_macro;
extern crate proc_macro2;

#[macro_use] extern crate syn;
#[macro_use] extern crate quote;

use proc_macro::TokenStream;
use syn::synom::Synom;
use syn::spanned::Spanned;
use syn::{ Ident, Type, LitStr, PathArguments, Attribute, ImplItem, ItemImpl };
use syn::punctuated::Punctuated;

struct GlobalFlag {
    ident: Ident,
    colon_token: Token![:],
    ty: Box<Type>,
    colon_token2: Option<Token![:]>,
    desc: Box<LitStr>
}

struct GlobalFlags {
    inner: Punctuated<GlobalFlag, Token![,]>
}

#[derive(Debug)]
struct Doc {
    eq: Token![=],
    desc: LitStr
}

impl Synom for GlobalFlag {
    named!(parse -> Self, do_parse!(
            ident: syn!(Ident) >>
            colon_token: punct!(:) >>
            ty: syn!(Type) >>
            colon_token2: option!(punct!(:)) >>
            desc: syn!(LitStr) >>
            (GlobalFlag {
                ident, colon_token, ty: Box::new(ty), colon_token2, desc: Box::new(desc)
            })
            ));

    fn description() -> Option<&'static str> {
        Some("global flag")
    }
}

impl Synom for GlobalFlags {
    named!(parse -> Self, do_parse!(
            inner: call!(Punctuated::parse_terminated) >>
            (GlobalFlags {
                inner
            })
            ));

    fn description() -> Option<&'static str> {
        Some("global flags")
    }
}

impl Synom for Doc {
    named!(parse -> Self, do_parse!(
            eq: syn!(syn::token::Eq) >>
            desc: syn!(LitStr) >>
            (Doc {
                eq, desc
            })
            ));
}

fn name_from_type_path(ty: &Type) -> String {
    match ty {
        Type::Path(p) => p.path.segments
            .iter()
            .fold(String::new(), |acc, ps| {
                if acc.len() == 0 {
                    format!("{}", ps.ident)
                } else {
                    format!("{}::{}", acc, ps.ident)
                }
            }),
        Type::Reference(tr) => {
            name_from_type_path(&tr.elem)
        },
        _ => unimplemented!()
    }
}

fn extract_about(attrs: &Vec<Attribute>) -> String {
    attrs
        .iter()
        .map(|x| syn::parse::<Doc>(x.tts.clone().into()))
        .filter(|x| x.is_ok())
        .map(Result::unwrap)
        .fold(String::new(), |acc, x| {
            if acc.len() == 0 {
                x.desc.value()
            } else {
                format!("{}\n{}", acc, x.desc.value())
            }
        })
}

fn extract_inner_type(ty: &Type) -> proc_macro2::TokenStream {
    let mut quote_ty = quote!(#ty);
    match *ty {
        Type::Path(ref p) => {
            let last_segment = p.path.segments.last().unwrap();
            let last_segment_value = last_segment.value();
            if name_from_type_path(&ty) == "Option" {
                if let PathArguments::AngleBracketed(ref a) = last_segment_value.arguments {
                    if let syn::GenericArgument::Type(t) = a.args.first().unwrap().value() {
                        let inner_type = extract_inner_type(&t);
                        quote_ty = quote!(#inner_type);
                    }
                } else {
                    ty.span().unstable().error("Type not supported by thunder").emit();
                }
            }
        },
        Type::Reference(ref tr) => {
            if tr.mutability.is_some() {
                ty.span()
                    .unstable()
                    .error("Thunder does not support mutable arguments")
                    .emit()
            }

            if tr.lifetime.is_some() {
                ty.span()
                    .unstable()
                    .error("Thunder does not support lifetime on arguments")
                    .emit()
            }

            let inner = extract_inner_type(&tr.elem);
            quote_ty = quote! {
                &#inner
            };
        },
        _ => {
            ty.span()
                .unstable()
                .error("Type not supported by thunder")
                .emit();
        }
    }

    return quote_ty;
}

#[proc_macro_attribute]
pub fn experiment(args: TokenStream, input: TokenStream) -> TokenStream {
    let item_impl: ItemImpl = syn::parse(input).expect("Failed to parse input for experiment macro");
    let global_flags: GlobalFlags = syn::parse(args).expect("Failed to parse global flags");

    let impl_name = &item_impl.self_ty;

    let mut accessors = quote!();
    let mut init_data_struct = quote!();
    let mut data_struct = quote!();
    let mut global_matcher = quote!();

    let impl_name_str = name_from_type_path(impl_name);
    let about = extract_about(&item_impl.attrs);
    let trim_about = about.trim();
    let mut app = quote! {
        App::new(#impl_name_str)
            .setting(AppSettings::SubcommandRequired)
            .about(#trim_about)
    };

    global_flags
        .inner
        .into_iter()
        .for_each(|GlobalFlag { ident, colon_token, ty, colon_token2, desc }| {
            let quote_ty = extract_inner_type(&ty);
            let optional = name_from_type_path(&ty) == "Option";

            accessors = quote! {
                #accessors

                fn #ident() -> #ty {
                    unsafe {
                        __THUNDER_DATA_STATIC.as_ref().unwrap().#ident.as_ref().unwrap().clone()
                    }
                }
            };

            init_data_struct = quote! {
                #init_data_struct
                #ident: None,
            };

            data_struct = quote! {
                #data_struct
                #ident: Option<#ty>,
            };


            let name = format!("{}", ident);
            let desc = format!("{}", desc.value());
            let span = ty.span();
            global_matcher = if optional {
                quote_spanned!{span=>
                    #global_matcher
                    global_match_states.#ident = Some(args.value_of(#name).map(|x| x.parse::< #quote_ty >().expect("Failed to parse value. Double check!")));
                }
            } else {
                quote_spanned!{span=>
                    #global_matcher
                    global_match_states.#ident = Some(args.value_of(#name).map(|x| x.parse::< #quote_ty >().expect("Failed to parse value. Double check!")).unwrap());
                }
            };

            app = if optional {
                let long = format!("--{}", name);
                let short = format!("-{}", &name[..1]);
                quote! {
                    #app
                    .arg(Arg::with_name(#name).long(#long).short(#short).takes_value(true).help(#desc))
                }
            } else {
                quote! {
                    #app
                    .arg(Arg::with_name(#name).takes_value(true).required(true).help(#desc))
                }
            };
        });

    item_impl
        .items
        .iter()
        .map(|item| {
            if let ImplItem::Method(method) = item {
                Some(method)
            } else {
                None
            }
        })
        .filter(Option::is_some)
        .for_each(|item| {
            let item = item.unwrap();
            let about = extract_about(&item.attrs);
            let subcommand = &item.sig.ident;
            let arguments = quote!();

            item.sig
                .decl
                .inputs
                .iter()
                .map(|input| {
                    if let syn::FnArg::Captured(arg) = input {
                        Some(arg)
                    } else {
                        input
                            .span()
                            .unstable()
                            .error("Thunder does not support this function argument")
                            .emit();
                        None
                    }
                })
                .filter(Option::is_some)
                .for_each(|input| {
                    let input = input.unwrap();
                    println!("input.ty = {}", extract_inner_type(&input.ty));
                });

            println!("{:?}", item);
        });

    let mut matchy = quote!();
    matchy = quote! {
        let mut global_match_states = __ThunderDataStaticStore::new_empty_store();
        #global_matcher

        unsafe {
            __THUNDER_DATA_STATIC = Some(global_match_states);
        }

        #matchy
    };

    (quote! {
        #item_impl

        impl #impl_name {
            fn start() {
                use clap::{App, SubCommand, Arg, AppSettings};

                let app = #app;
                let args = app.get_matches();

                #matchy
            }

            #accessors
        }

        static mut __THUNDER_DATA_STATIC: Option<__ThunderDataStaticStore> = None;

        /// This block was generated by thunder v0.0.0
        #[allow(unused)]
        #[doc(hidden)]
        struct __ThunderDataStaticStore {
            #data_struct
        }

        #[allow(unused)]
        #[doc(hidden)]
        impl __ThunderDataStaticStore {
            pub fn new_empty_store() -> __ThunderDataStaticStore {
                __ThunderDataStaticStore {
                    #init_data_struct
                }
            }
        }
    }).into()
}
