// SPDX-License-Identifier: Apache-2.0 OR MIT

#![allow(clippy::needless_pass_by_value, clippy::redundant_guards, clippy::wildcard_imports)]

#[macro_use]
mod file;

use std::{collections::HashSet, path::Path};

use fs_err as fs;
use quote::{format_ident, quote};
use syn::{punctuated::Punctuated, *};

use crate::file::*;

fn main() {
    gen_de();
    gen_is_none();
    gen_assert_impl();
    gen_track_size();
}

fn gen_de() {
    const FILES: &[&str] = &["src/de.rs"];
    const MERGE_EXCLUDE: &[&str] = &[
        "de::CredentialProvider",
        "de::Flags",
        "de::EnvConfigValue",
        "de::StringList",
        "de::PathAndArgs",
    ];
    const SET_PATH_EXCLUDE: &[&str] = &[];

    let workspace_root = &workspace_root();

    let mut tokens = quote! {
        use std::path::Path;
        use crate::{
            error::Result,
            merge::Merge,
            value::SetPath,
        };
    };

    let mut visited_types = HashSet::new();
    for &f in FILES {
        let s = fs::read_to_string(workspace_root.join(f)).unwrap();
        let ast = syn::parse_file(&s).unwrap();

        let module = if f.ends_with("lib.rs") {
            vec![]
        } else {
            let name = format_ident!("{}", Path::new(f).file_stem().unwrap().to_string_lossy());
            vec![name.into()]
        };

        test_helper::codegen::visit_items(module, ast, |item, module| {
            // impl Merge
            match item {
                syn::Item::Struct(syn::ItemStruct { vis, ident, fields, .. })
                    if matches!(vis, syn::Visibility::Public(..))
                        && matches!(fields, syn::Fields::Named(..)) =>
                {
                    let path_string = quote! { #(#module::)* #ident }.to_string().replace(' ', "");
                    visited_types.insert(path_string.clone());
                    if !MERGE_EXCLUDE.contains(&path_string.as_str()) {
                        let fields = fields
                            .iter()
                            .filter(|f| {
                                !serde_skip(&f.attrs)
                                    && f.ident.as_ref().unwrap() != "serialized_repr"
                                    && f.ident.as_ref().unwrap() != "deserialized_repr"
                            })
                            .map(|syn::Field { ident, .. }| {
                                quote! { self.#ident.merge(low.#ident, force)?; }
                            });
                        tokens.extend(quote! {
                            impl Merge for crate:: #(#module::)* #ident {
                                fn merge(&mut self, low: Self, force: bool) -> Result<()> {
                                    #(#fields)*
                                    Ok(())
                                }
                            }
                        });
                    }
                }
                _ => {}
            }
            // impl SetPath
            match item {
                syn::Item::Struct(syn::ItemStruct { vis, ident, fields, .. })
                    if matches!(vis, syn::Visibility::Public(..)) =>
                {
                    let path_string = quote! { #(#module::)* #ident }.to_string().replace(' ', "");
                    visited_types.insert(path_string.clone());
                    if !SET_PATH_EXCLUDE.contains(&path_string.as_str()) {
                        match fields {
                            Fields::Named(fields) => {
                                let fields = fields
                                    .named
                                    .iter()
                                    .filter(|f| {
                                        !serde_skip(&f.attrs)
                                            && f.ident.as_ref().unwrap() != "serialized_repr"
                                            && f.ident.as_ref().unwrap() != "deserialized_repr"
                                    })
                                    .map(|syn::Field { ident, .. }| {
                                        quote! { self.#ident.set_path(path); }
                                    });
                                tokens.extend(quote! {
                                    impl SetPath for crate:: #(#module::)* #ident {
                                        fn set_path(&mut self, path: &Path) {
                                            #(#fields)*
                                        }
                                    }
                                });
                            }
                            Fields::Unnamed(fields) => {
                                assert_eq!(fields.unnamed.len(), 1);
                                tokens.extend(quote! {
                                    impl SetPath for crate:: #(#module::)* #ident {
                                        fn set_path(&mut self, path: &Path) {
                                            self.0.set_path(path);
                                        }
                                    }
                                });
                            }
                            Fields::Unit => unreachable!(),
                        }
                    }
                }
                syn::Item::Enum(syn::ItemEnum { vis, ident, variants, .. })
                    if matches!(vis, syn::Visibility::Public(..))
                        && variants.iter().all(|v| !v.fields.is_empty()) =>
                {
                    let path_string = quote! { #(#module::)* #ident }.to_string().replace(' ', "");
                    visited_types.insert(path_string.clone());
                    if !SET_PATH_EXCLUDE.contains(&path_string.as_str()) {
                        let mut arms = Vec::with_capacity(variants.len());
                        for syn::Variant { ident, fields, .. } in variants {
                            match fields {
                                Fields::Named(fields) => {
                                    let pat = fields
                                        .named
                                        .iter()
                                        .filter(|f| !serde_skip(&f.attrs))
                                        .map(|syn::Field { ident, .. }| ident);
                                    let calls =
                                        fields.named.iter().filter(|f| !serde_skip(&f.attrs)).map(
                                            |syn::Field { ident, .. }| {
                                                quote! { #ident.set_path(path); }
                                            },
                                        );
                                    arms.push(quote! {
                                        Self::#ident { #(#pat),* } => {
                                            #(#calls)*
                                        }
                                    });
                                }
                                Fields::Unnamed(fields) => {
                                    assert_eq!(fields.unnamed.len(), 1);
                                    arms.push(quote! {
                                        Self::#ident(v) => {
                                            v.set_path(path);
                                        }
                                    });
                                }
                                Fields::Unit => unreachable!(),
                            }
                        }
                        tokens.extend(quote! {
                            impl SetPath for crate:: #(#module::)* #ident {
                                fn set_path(&mut self, path: &Path) {
                                    match self {
                                        #(#arms,)*
                                    }
                                }
                            }
                        });
                    }
                }
                _ => {}
            }
        });
    }

    for &t in MERGE_EXCLUDE {
        assert!(
            visited_types.contains(t),
            "unknown type `{t}` specified in MERGE_EXCLUDE constant"
        );
    }
    for &t in SET_PATH_EXCLUDE {
        assert!(
            visited_types.contains(t),
            "unknown type `{t}` specified in SET_PATH_EXCLUDE constant"
        );
    }

    write(function_name!(), workspace_root.join("src/gen/de.rs"), tokens).unwrap();
}

fn gen_is_none() {
    const FILES: &[&str] = &["src/lib.rs", "src/easy.rs", "src/de.rs"];
    const EXCLUDE: &[&str] = &[
        "de::Config",
        "de::CredentialProvider",
        "de::Flags",
        "de::PathAndArgs",
        "de::StringList",
        "de::TargetConfig",
        "de::RegistriesConfigValue",
        "easy::Config",
        "easy::EnvConfigValue",
        "easy::Flags",
        "easy::PathAndArgs",
        "easy::StringList",
        "easy::TargetConfig",
        "easy::RegistriesConfigValue",
    ];

    let workspace_root = &workspace_root();

    let mut tokens = quote! {};

    let mut visited_types = HashSet::new();
    for &f in FILES {
        let s = fs::read_to_string(workspace_root.join(f)).unwrap();
        let ast = syn::parse_file(&s).unwrap();

        let module = if f.ends_with("lib.rs") {
            vec![]
        } else {
            let name = format_ident!("{}", Path::new(f).file_stem().unwrap().to_string_lossy());
            vec![name.into()]
        };

        test_helper::codegen::visit_items(module, ast, |item, module| match item {
            syn::Item::Struct(syn::ItemStruct { vis, ident, fields, .. })
                if matches!(vis, syn::Visibility::Public(..))
                    && matches!(fields, syn::Fields::Named(..)) =>
            {
                let path_string = quote! { #(#module::)* #ident }.to_string().replace(' ', "");
                visited_types.insert(path_string.clone());
                if !EXCLUDE.contains(&path_string.as_str()) {
                    let fields = fields.iter().filter(|f| !serde_skip(&f.attrs)).map(
                        |syn::Field { ident, .. }| {
                            quote! { self.#ident.is_none() }
                        },
                    );
                    tokens.extend(quote! {
                        impl crate:: #(#module::)* #ident {
                            pub(crate) fn is_none(&self) -> bool {
                                #(#fields) &&*
                            }
                        }
                    });
                }
            }
            _ => {}
        });
    }

    for &t in EXCLUDE {
        assert!(visited_types.contains(t), "unknown type `{t}` specified in EXCLUDE constant");
    }

    write(function_name!(), workspace_root.join("src/gen/is_none.rs"), tokens).unwrap();
}

fn serde_skip(attrs: &[syn::Attribute]) -> bool {
    for meta in attrs
        .iter()
        .filter(|attr| attr.path().is_ident("serde"))
        .filter_map(|attr| {
            attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated).ok()
        })
        .flatten()
    {
        if let syn::Meta::Path(p) = meta {
            if p.is_ident("skip") {
                return true;
            }
        }
    }
    false
}

fn gen_assert_impl() {
    let (path, out) = test_helper::codegen::gen_assert_impl(
        &workspace_root(),
        test_helper::codegen::AssertImplConfig {
            exclude: &[],
            not_send: &[],
            not_sync: &["easy::Config", "resolve::ResolveContext"],
            not_unpin: &[],
            not_unwind_safe: &["error::Error"],
            not_ref_unwind_safe: &["error::Error", "easy::Config", "resolve::ResolveContext"],
        },
    );
    write(function_name!(), path, out).unwrap();
}

fn gen_track_size() {
    let (path, out) = test_helper::codegen::gen_track_size(
        &workspace_root(),
        test_helper::codegen::TrackSizeConfig { exclude: &[] },
    );
    write(function_name!(), path, out).unwrap();
}
