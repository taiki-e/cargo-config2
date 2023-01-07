#![warn(rust_2018_idioms, single_use_lifetimes)]

mod file;

use std::{collections::BTreeSet, path::Path};

use anyhow::Result;
use fs_err as fs;
use quote::{format_ident, quote};
use syn::{
    visit_mut::{self, VisitMut},
    *,
};

use crate::file::*;

fn main() -> Result<()> {
    gen_assert_impl()?;
    gen_de()?;
    gen_is_none()?;
    Ok(())
}

fn gen_de() -> Result<()> {
    const FILES: &[&str] = &["src/de.rs"];
    // TODO: check if this list is outdated
    const MERGE_EXCLUDE: &[&str] =
        &["Rustflags", "ResolveContext", "EnvConfigValue", "StringList", "PathAndArgs"];
    const SET_PATH_EXCLUDE: &[&str] = &[];

    let workspace_root = &workspace_root();

    let mut tokens = quote! {
        use std::path::Path;
        use crate::{
            merge::Merge,
            value::SetPath,
            Result,
        };
    };

    for &f in FILES {
        let s = fs::read_to_string(workspace_root.join(f))?;
        let mut ast = syn::parse_file(&s)?;

        let module = if f.ends_with("lib.rs") {
            vec![]
        } else {
            let name = format_ident!("{}", Path::new(f).file_stem().unwrap().to_string_lossy());
            vec![name.into()]
        };

        ItemVisitor::new(module, |item, module| {
            // impl Merge
            match item {
                syn::Item::Struct(syn::ItemStruct { vis, ident, fields, .. })
                    if matches!(vis, syn::Visibility::Public(..))
                        && matches!(fields, syn::Fields::Named(..))
                        && !MERGE_EXCLUDE.iter().any(|&e| ident == e) =>
                {
                    let fields = fields
                        .iter()
                        .filter(|f| {
                            !serde_skip(&f.attrs)
                                && f.ident.as_ref().unwrap() != "serialized_repr"
                                && f.ident.as_ref().unwrap() != "deserialized_repr"
                        })
                        .map(|syn::Field { ident, .. }| {
                            quote! { self.#ident.merge(from.#ident, force)?; }
                        });
                    tokens.extend(quote! {
                        impl Merge for crate:: #(#module::)* #ident {
                            fn merge(&mut self, from: Self, force: bool) -> Result<()> {
                                #(#fields)*
                                Ok(())
                            }
                        }
                    });
                }
                _ => {}
            }
            // impl SetPath
            match item {
                syn::Item::Struct(syn::ItemStruct { vis, ident, fields, .. })
                    if matches!(vis, syn::Visibility::Public(..))
                        && !SET_PATH_EXCLUDE.iter().any(|&e| ident == e) =>
                {
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
                syn::Item::Enum(syn::ItemEnum { vis, ident, variants, .. })
                    if matches!(vis, syn::Visibility::Public(..))
                        && variants.iter().all(|v| !v.fields.is_empty())
                        && SET_PATH_EXCLUDE.iter().all(|&e| ident != e) =>
                {
                    let mut arms = vec![];
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
                _ => {}
            }
        })
        .visit_file_mut(&mut ast);
    }

    write("gen_de", &workspace_root.join("src/gen/de.rs"), tokens)?;

    Ok(())
}

fn gen_is_none() -> Result<()> {
    const FILES: &[&str] = &["src/lib.rs", "src/easy.rs", "src/de.rs"];
    // TODO: check if this list is outdated
    const EXCLUDE: &[&str] = &[
        "Config",
        "TargetConfig",
        "Rustflags",
        "ResolveContext",
        "EnvConfigValue",
        "StringList",
        "PathAndArgs",
    ];

    let workspace_root = &workspace_root();

    let mut tokens = quote! {};

    for &f in FILES {
        let s = fs::read_to_string(workspace_root.join(f))?;
        let mut ast = syn::parse_file(&s)?;

        let module = if f.ends_with("lib.rs") {
            vec![]
        } else {
            let name = format_ident!("{}", Path::new(f).file_stem().unwrap().to_string_lossy());
            vec![name.into()]
        };

        ItemVisitor::new(module, |item, module| match item {
            syn::Item::Struct(syn::ItemStruct { vis, ident, fields, .. })
                if matches!(vis, syn::Visibility::Public(..))
                    && matches!(fields, syn::Fields::Named(..))
                    && !EXCLUDE.iter().any(|&e| ident == e) =>
            {
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
            _ => {}
        })
        .visit_file_mut(&mut ast);
    }

    write("gen_is_none", &workspace_root.join("src/gen/is_none.rs"), tokens)?;

    Ok(())
}

fn serde_skip(attrs: &[syn::Attribute]) -> bool {
    for meta in attrs
        .iter()
        .filter(|attr| attr.path.is_ident("serde"))
        .filter_map(|attr| attr.parse_meta().ok())
    {
        if let syn::Meta::List(list) = meta {
            for repr in list.nested {
                if let syn::NestedMeta::Meta(syn::Meta::Path(p)) = repr {
                    if p.is_ident("skip") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn gen_assert_impl() -> Result<()> {
    let workspace_root = &workspace_root();
    let out_dir = &workspace_root.join("src/gen");
    fs::create_dir_all(out_dir)?;

    let files: BTreeSet<String> = ignore::Walk::new(workspace_root.join("src"))
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            if !path.is_file() || path.extension() != Some("rs".as_ref()) {
                return None;
            }
            // Assertions are only needed for the library's public APIs.
            if path.ends_with("main.rs") {
                return None;
            }
            Some(path.to_string_lossy().into_owned())
        })
        .collect();

    let mut tokens = quote! {};
    for f in &files {
        let s = fs::read_to_string(f)?;
        let mut ast = syn::parse_file(&s)?;

        let module = if f.ends_with("lib.rs") {
            vec![]
        } else {
            let name = format_ident!("{}", Path::new(f).file_stem().unwrap().to_string_lossy());
            vec![name.into()]
        };

        ItemVisitor::new(module, |item, module| match item {
            syn::Item::Struct(syn::ItemStruct { vis, ident, generics, .. })
            | syn::Item::Enum(syn::ItemEnum { vis, ident, generics, .. })
            | syn::Item::Union(syn::ItemUnion { vis, ident, generics, .. })
            | syn::Item::Type(syn::ItemType { vis, ident, generics, .. })
                if matches!(vis, syn::Visibility::Public(..)) =>
            {
                // TODO: handle generics
                if generics.type_params().count() != 0 || generics.const_params().count() != 0 {
                    return;
                }

                let lt_count = generics.lifetimes().count();
                let lt = if lt_count > 0 {
                    let lt = (0..lt_count).map(|_| quote! { '_ });
                    quote! { <#(#lt),*> }
                } else {
                    quote! {}
                };
                tokens.extend(quote! {
                    assert_auto_traits::<crate:: #(#module::)* #ident #lt>();
                });
            }
            _ => {}
        })
        .visit_file_mut(&mut ast);
    }

    let out = quote! {
        const _: fn() = || {
            fn assert_auto_traits<T: ?Sized + Send + Sync + Unpin>() {}
            #tokens
        };
    };
    write("gen_assert_impl", &out_dir.join("assert_impl.rs"), out)?;

    Ok(())
}

struct ItemVisitor<F> {
    module: Vec<syn::PathSegment>,
    f: F,
}

impl<F> ItemVisitor<F>
where
    F: FnMut(&mut syn::Item, &[syn::PathSegment]),
{
    fn new(module: Vec<syn::PathSegment>, f: F) -> Self {
        Self { module, f }
    }
}

impl<F> VisitMut for ItemVisitor<F>
where
    F: FnMut(&mut syn::Item, &[syn::PathSegment]),
{
    fn visit_item_mut(&mut self, item: &mut syn::Item) {
        if let syn::Item::Mod(item) = item {
            self.module.push(item.ident.clone().into());
            visit_mut::visit_item_mod_mut(self, item);
            self.module.pop();
            return;
        }
        (self.f)(item, &self.module);
        visit_mut::visit_item_mut(self, item);
    }
}
