// SPDX-License-Identifier: Apache-2.0 OR MIT

#![allow(clippy::needless_pass_by_value, clippy::redundant_guards, clippy::wildcard_imports)]

#[macro_use]
mod file;

use std::{
    collections::{BTreeSet, HashSet},
    path::Path,
};

use anyhow::Result;
use fs_err as fs;
use quote::{format_ident, quote, ToTokens};
use syn::{punctuated::Punctuated, visit_mut::VisitMut, *};

use crate::file::*;

fn main() -> Result<()> {
    gen_assert_impl()?;
    gen_de()?;
    gen_is_none()?;
    Ok(())
}

fn gen_de() -> Result<()> {
    const FILES: &[&str] = &["src/de.rs"];
    const MERGE_EXCLUDE: &[&str] =
        &["de::Flags", "de::EnvConfigValue", "de::StringList", "de::PathAndArgs"];
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
        })
        .visit_file_mut(&mut ast);
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

    write(function_name!(), workspace_root.join("src/gen/de.rs"), tokens)?;

    Ok(())
}

fn gen_is_none() -> Result<()> {
    const FILES: &[&str] = &["src/lib.rs", "src/easy.rs", "src/de.rs"];
    const EXCLUDE: &[&str] = &[
        "de::Config",
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
        })
        .visit_file_mut(&mut ast);
    }

    for &t in EXCLUDE {
        assert!(visited_types.contains(t), "unknown type `{t}` specified in EXCLUDE constant");
    }

    write(function_name!(), workspace_root.join("src/gen/is_none.rs"), tokens)?;

    Ok(())
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

fn gen_assert_impl() -> Result<()> {
    const NOT_SEND: &[&str] = &[];
    const NOT_SYNC: &[&str] = &["easy::Config", "resolve::ResolveContext"];
    const NOT_UNPIN: &[&str] = &[];
    const NOT_UNWIND_SAFE: &[&str] = &["error::Error"];
    const NOT_REF_UNWIND_SAFE: &[&str] =
        &["error::Error", "easy::Config", "resolve::ResolveContext"];

    let workspace_root = &workspace_root();
    let out_dir = &workspace_root.join("src/gen");
    fs::create_dir_all(out_dir)?;

    let files: BTreeSet<String> = git_ls_files(&workspace_root.join("src"), &["*.rs"])?
        .into_iter()
        .filter_map(|(file_name, path)| {
            // Assertions are only needed for the library's public APIs.
            if file_name == "main.rs" || file_name.starts_with("bin/") {
                return None;
            }
            Some(path.to_string_lossy().into_owned())
        })
        .collect();

    let mut tokens = quote! {};
    let mut visited_types = HashSet::new();
    let mut use_generics_helpers = false;
    for f in &files {
        let s = fs::read_to_string(f)?;
        let mut ast = syn::parse_file(&s)?;

        let module = if f.ends_with("lib.rs") {
            vec![]
        } else {
            let name = format_ident!("{}", Path::new(f).file_stem().unwrap().to_string_lossy());
            vec![name.into()]
        };

        // TODO: assert impl trait returned from public functions
        ItemVisitor::new(module, |item, module| match item {
            syn::Item::Struct(syn::ItemStruct { vis, ident, generics, .. })
            | syn::Item::Enum(syn::ItemEnum { vis, ident, generics, .. })
            | syn::Item::Union(syn::ItemUnion { vis, ident, generics, .. })
            | syn::Item::Type(syn::ItemType { vis, ident, generics, .. })
                if matches!(vis, syn::Visibility::Public(..)) =>
            {
                let path_string = quote! { #(#module::)* #ident }.to_string().replace(' ', "");
                visited_types.insert(path_string.clone());

                let has_generics = generics.type_params().count() != 0;
                let has_lifetimes = generics.lifetimes().count() != 0;
                assert_eq!(
                    generics.const_params().count(),
                    0,
                    "gen_assert_impl doesn't support const generics yet; skipped `{path_string}`"
                );

                let lt = generics.lifetimes().map(|_| quote! { '_ });
                if has_generics {
                    let lt = quote! { #(#lt,)* };
                    use_generics_helpers = true;
                    // Send & Sync & Unpin & UnwindSafe & RefUnwindSafe
                    let unit = generics.type_params().map(|_| quote! { () });
                    let unit_generics = quote! { <#lt #(#unit),*> };
                    // !Send & Sync
                    let not_send = generics.type_params().map(|_| quote! { NotSend });
                    let not_send_generics = quote! { <#lt #(#not_send),*> };
                    // Send & !Sync
                    let not_sync = generics.type_params().map(|_| quote! { NotSync });
                    let not_sync_generics = quote! { <#lt #(#not_sync),*> };
                    // !Unpin
                    let not_unpin = generics.type_params().map(|_| quote! { NotUnpin });
                    let not_unpin_generics = quote! { <#lt #(#not_unpin),*> };
                    // !UnwindSafe
                    let not_unwind_safe = generics.type_params().map(|_| quote! { NotUnwindSafe });
                    let not_unwind_safe_generics = quote! { <#lt #(#not_unwind_safe),*> };
                    // !RefUnwindSafe
                    let not_ref_unwind_safe =
                        generics.type_params().map(|_| quote! { NotRefUnwindSafe });
                    let not_ref_unwind_safe_generics = quote! { <#lt #(#not_ref_unwind_safe),*> };
                    if NOT_SEND.contains(&path_string.as_str()) {
                        tokens.extend(quote! {
                            assert_not_send!(crate:: #(#module::)* #ident #unit_generics);
                        });
                    } else {
                        tokens.extend(quote! {
                            assert_send::<crate:: #(#module::)* #ident #unit_generics>();
                            assert_send::<crate:: #(#module::)* #ident #not_sync_generics>();
                            assert_not_send!(crate:: #(#module::)* #ident #not_send_generics);
                        });
                    }
                    if NOT_SYNC.contains(&path_string.as_str()) {
                        tokens.extend(quote! {
                            assert_not_sync!(crate:: #(#module::)* #ident #unit_generics);
                        });
                    } else {
                        tokens.extend(quote! {
                            assert_sync::<crate:: #(#module::)* #ident #unit_generics>();
                            assert_sync::<crate:: #(#module::)* #ident #not_send_generics>();
                            assert_not_sync!(crate:: #(#module::)* #ident #not_sync_generics);
                        });
                    }
                    if NOT_UNPIN.contains(&path_string.as_str()) {
                        tokens.extend(quote! {
                            assert_not_unpin!(crate:: #(#module::)* #ident #unit_generics);
                        });
                    } else {
                        tokens.extend(quote! {
                            assert_unpin::<crate:: #(#module::)* #ident #unit_generics>();
                            assert_not_unpin!(crate:: #(#module::)* #ident #not_unpin_generics);
                        });
                    }
                    if NOT_UNWIND_SAFE.contains(&path_string.as_str()) {
                        tokens.extend(quote! {
                            assert_not_unwind_safe!(crate:: #(#module::)* #ident #unit_generics);
                        });
                    } else {
                        tokens.extend(quote! {
                            assert_unwind_safe::<crate:: #(#module::)* #ident #unit_generics>();
                            assert_not_unwind_safe!(
                                crate:: #(#module::)* #ident #not_unwind_safe_generics
                            );
                        });
                    }
                    if NOT_REF_UNWIND_SAFE.contains(&path_string.as_str()) {
                        tokens.extend(quote! {
                            assert_not_ref_unwind_safe!(
                                crate:: #(#module::)* #ident #unit_generics
                            );
                        });
                    } else {
                        tokens.extend(quote! {
                            assert_ref_unwind_safe::<crate:: #(#module::)* #ident #unit_generics>();
                            assert_not_ref_unwind_safe!(
                                crate:: #(#module::)* #ident #not_ref_unwind_safe_generics
                            );
                        });
                    }
                } else {
                    let lt = if has_lifetimes {
                        quote! { <#(#lt),*> }
                    } else {
                        quote! {}
                    };
                    if NOT_SEND.contains(&path_string.as_str()) {
                        tokens.extend(quote! {
                            assert_not_send!(crate:: #(#module::)* #ident #lt);
                        });
                    } else {
                        tokens.extend(quote! {
                            assert_send::<crate:: #(#module::)* #ident #lt>();
                        });
                    }
                    if NOT_SYNC.contains(&path_string.as_str()) {
                        tokens.extend(quote! {
                            assert_not_sync!(crate:: #(#module::)* #ident #lt);
                        });
                    } else {
                        tokens.extend(quote! {
                            assert_sync::<crate:: #(#module::)* #ident #lt>();
                        });
                    }
                    if NOT_UNPIN.contains(&path_string.as_str()) {
                        tokens.extend(quote! {
                            assert_not_unpin!(crate:: #(#module::)* #ident #lt);
                        });
                    } else {
                        tokens.extend(quote! {
                            assert_unpin::<crate:: #(#module::)* #ident #lt>();
                        });
                    }
                    if NOT_UNWIND_SAFE.contains(&path_string.as_str()) {
                        tokens.extend(quote! {
                            assert_not_unwind_safe!(crate:: #(#module::)* #ident #lt);
                        });
                    } else {
                        tokens.extend(quote! {
                            assert_unwind_safe::<crate:: #(#module::)* #ident #lt>();
                        });
                    }
                    if NOT_REF_UNWIND_SAFE.contains(&path_string.as_str()) {
                        tokens.extend(quote! {
                            assert_not_ref_unwind_safe!(crate:: #(#module::)* #ident #lt);
                        });
                    } else {
                        tokens.extend(quote! {
                            assert_ref_unwind_safe::<crate:: #(#module::)* #ident #lt>();
                        });
                    }
                };
            }
            _ => {}
        })
        .visit_file_mut(&mut ast);
    }

    let mut use_macros = use_generics_helpers;
    for (list, name) in &[
        (NOT_SEND, "NOT_SEND"),
        (NOT_SYNC, "NOT_SYNC"),
        (NOT_UNPIN, "NOT_UNPIN"),
        (NOT_UNWIND_SAFE, "NOT_UNWIND_SAFE"),
        (NOT_REF_UNWIND_SAFE, "NOT_REF_UNWIND_SAFE"),
    ] {
        use_macros |= !list.is_empty();
        for &ty in *list {
            assert!(visited_types.contains(ty), "unknown type `{ty}` specified in {name} constant");
        }
    }

    let mut out = quote! {
        #![allow(
            dead_code,
            unused_macros,
            clippy::std_instead_of_alloc,
            clippy::std_instead_of_core,
        )]
        fn assert_send<T: ?Sized + Send>() {}
        fn assert_sync<T: ?Sized + Sync>() {}
        fn assert_unpin<T: ?Sized + Unpin>() {}
        fn assert_unwind_safe<T: ?Sized + std::panic::UnwindSafe>() {}
        fn assert_ref_unwind_safe<T: ?Sized + std::panic::RefUnwindSafe>() {}
    };
    if use_generics_helpers {
        out.extend(quote! {
            /// `Send` & `!Sync`
            struct NotSync(core::cell::UnsafeCell<()>);
            /// `!Send` & `Sync`
            struct NotSend(std::sync::MutexGuard<'static, ()>);
            /// `!Send` & `!Sync`
            struct NotSendSync(*const ());
            /// `!Unpin`
            struct NotUnpin(core::marker::PhantomPinned);
            /// `!UnwindSafe`
            struct NotUnwindSafe(&'static mut ());
            /// `!RefUnwindSafe`
            struct NotRefUnwindSafe(core::cell::UnsafeCell<()>);
        });
    }
    if use_macros {
        out.extend(quote! {
            macro_rules! assert_not_send {
                ($ty:ty) => {
                    static_assertions::assert_not_impl_all!($ty: Send);
                };
            }
            macro_rules! assert_not_sync {
                ($ty:ty) => {
                    static_assertions::assert_not_impl_all!($ty: Sync);
                };
            }
            macro_rules! assert_not_unpin {
                ($ty:ty) => {
                    static_assertions::assert_not_impl_all!($ty: Unpin);
                };
            }
            macro_rules! assert_not_unwind_safe {
                ($ty:ty) => {
                    static_assertions::assert_not_impl_all!($ty: std::panic::UnwindSafe);
                };
            }
            macro_rules! assert_not_ref_unwind_safe {
                ($ty:ty) => {
                    static_assertions::assert_not_impl_all!($ty: std::panic::RefUnwindSafe);
                };
            }
        });
    }
    out.extend(quote! {
        const _: fn() = || {
            #tokens
        };
    });
    write(function_name!(), out_dir.join("assert_impl.rs"), out)?;

    Ok(())
}

#[must_use]
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
        match item {
            syn::Item::Mod(item) => {
                self.module.push(item.ident.clone().into());
                visit_mut::visit_item_mod_mut(self, item);
                self.module.pop();
            }
            syn::Item::Macro(item) => {
                if let Ok(mut file) = syn::parse2::<syn::File>(item.mac.tokens.clone()) {
                    visit_mut::visit_file_mut(self, &mut file);
                    item.mac.tokens = file.into_token_stream();
                }
                visit_mut::visit_item_macro_mut(self, item);
            }
            _ => {
                (self.f)(item, &self.module);
                visit_mut::visit_item_mut(self, item);
            }
        }
    }
}
