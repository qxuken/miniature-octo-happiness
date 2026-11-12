use quote::{quote, quote_spanned};
use syn::{Data, DeriveInput, Fields, LitStr, Meta, Path, spanned::Spanned};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let mut crate_path: Path = syn::parse_quote!(::weekend_db);
    let mut crate_seen = false;
    for attr in &input.attrs {
        if attr.path().is_ident("resolve") {
            return Err(syn::Error::new(attr.span(), "put #[resolve] on a field"));
        }
        if attr.path().is_ident("resolution") {
            attr.parse_nested_meta(|meta| {
                if !meta.path.is_ident("crate") {
                    return Err(meta.error("expected crate"));
                }
                if crate_seen {
                    return Err(meta.error("duplicate crate path"));
                }
                crate_seen = true;
                crate_path = meta.value()?.parse::<LitStr>()?.parse()?;
                Ok(())
            })?;
        }
    }

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "Resolution requires named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new(
                name.span(),
                "Resolution supports structs only",
            ));
        }
    };

    let mut selected = Vec::new();
    for field in fields {
        let mut resolve = None;
        for attr in &field.attrs {
            if attr.path().is_ident("resolution") {
                return Err(syn::Error::new(
                    attr.span(),
                    "put #[resolution(crate = ...)] on the struct",
                ));
            }
            if attr.path().is_ident("resolve") {
                if !matches!(attr.meta, Meta::Path(_)) {
                    return Err(syn::Error::new(attr.span(), "expected #[resolve]"));
                }
                if resolve.replace(attr.span()).is_some() {
                    return Err(syn::Error::new(attr.span(), "duplicate #[resolve]"));
                }
            }
        }
        if let Some(span) = resolve {
            selected.push((field, span));
        }
    }
    if selected.is_empty() {
        return Err(syn::Error::new(name.span(), "mark a field with #[resolve]"));
    }

    let crdt = quote!(#crate_path::crdt);
    let calls = selected.iter().map(|(field, span)| {
        let field = &field.ident;
        quote_spanned!(*span=> #crdt::Resolution::resolve_fields(&mut self.#field, actor)?;)
    });
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #crdt::Resolution for #name #type_generics #where_clause {
            fn resolve_fields(&mut self, actor: #crdt::Actor) -> #crate_path::Result<()> {
                #(#calls)*
                ::core::result::Result::Ok(())
            }
        }
    })
}

#[cfg(test)]
mod decision_tests {
    use super::*;
    #[test]
    fn resolution_selection_and_attribute_locations_are_checked() {
        for (source, message) in [
            ("enum A {}", "structs only"),
            ("struct A(u64);", "named fields"),
            ("struct A;", "named fields"),
            ("struct A {}", "mark a field"),
            (
                "#[resolution(crate=\"x\",crate=\"y\")] struct A { #[resolve] a:u64 }",
                "duplicate crate",
            ),
            (
                "#[resolution(crate=3)] struct A { #[resolve] a:u64 }",
                "string literal",
            ),
            (
                "struct A { #[resolution(crate=\"x\")] a:u64 }",
                "on the struct",
            ),
            ("struct A { #[resolve()] a:u64 }", "expected #[resolve]"),
            ("struct A { #[resolve=true] a:u64 }", "expected #[resolve]"),
        ] {
            let error = expand(syn::parse_str(source).unwrap()).unwrap_err();
            assert!(error.to_string().contains(message), "{source}: {error}");
        }
        for source in [
            "#[allow(dead_code)] struct A { #[resolve] a:u64, #[allow(dead_code)] b:u64 }",
            "#[resolution(crate=\"crate::api\")] struct A<T> where T: Copy { #[resolve] a:T, #[resolve] b:T }",
        ] {
            syn::parse2::<syn::File>(expand(syn::parse_str(source).unwrap()).unwrap()).unwrap();
        }
    }
}
