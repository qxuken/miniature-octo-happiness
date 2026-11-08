use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Expr, Fields, Ident, LitStr, Path, parse::Parse, spanned::Spanned};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let mut crate_path: Option<Path> = None;
    let mut key = None;
    let mut format = None;
    let mut resolve = None;
    let mut wire_order = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("record") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("fields") {
                if wire_order.is_some() {
                    return Err(meta.error("duplicate fields list"));
                }
                let fields;
                syn::parenthesized!(fields in meta.input);
                wire_order = Some(fields.parse_terminated(Ident::parse, syn::Token![,])?);
                return Ok(());
            }
            if meta.path.is_ident("crate") {
                if crate_path.is_some() {
                    return Err(meta.error("duplicate crate path"));
                }
                crate_path = Some(meta.value()?.parse::<LitStr>()?.parse()?);
                return Ok(());
            }
            let slot: &mut Option<Expr> = if meta.path.is_ident("key") {
                &mut key
            } else if meta.path.is_ident("format") {
                &mut format
            } else if meta.path.is_ident("resolve") {
                &mut resolve
            } else {
                return Err(meta.error("expected key, format, fields(...), resolve, or crate"));
            };
            if slot.is_some() {
                return Err(meta.error("duplicate record option"));
            }
            *slot = Some(meta.value()?.parse()?);
            Ok(())
        })?;
    }
    let crate_path = crate_path.unwrap_or_else(|| syn::parse_quote!(::weekend_db));
    let key = key.ok_or_else(|| syn::Error::new(name.span(), "specify record key"))?;
    let format = format.ok_or_else(|| syn::Error::new(name.span(), "specify record format"))?;
    let wire_order = wire_order.ok_or_else(|| {
        syn::Error::new(name.span(), "list fields in wire order with fields(...)")
    })?;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "Record requires named fields",
                ));
            }
        },
        _ => return Err(syn::Error::new(name.span(), "Record supports structs only")),
    };
    for field in fields {
        if let Some(attr) = field
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident("record"))
        {
            return Err(syn::Error::new(
                attr.span(),
                "put #[record(...)] on the struct",
            ));
        }
    }

    let mut names = Vec::new();
    let mut types = Vec::new();
    for name in &wire_order {
        if names.contains(&name) {
            return Err(syn::Error::new(
                name.span(),
                "duplicate field in wire order",
            ));
        }
        let field = fields
            .iter()
            .find(|field| field.ident.as_ref() == Some(name))
            .ok_or_else(|| syn::Error::new(name.span(), "unknown field in wire order"))?;
        names.push(name);
        types.push(&field.ty);
    }
    if names.len() != fields.len() {
        return Err(syn::Error::new(
            name.span(),
            "list every field exactly once in fields(...)",
        ));
    }

    let crdt = quote!(#crate_path::crdt);
    let decoded = (0..names.len())
        .map(|index| format_ident!("__weekend_field_{index}"))
        .collect::<Vec<_>>();
    let resolution = resolve.map(|enabled| {
        quote! {
            fn resolve_conflicts(&mut self, actor: #crdt::Actor) -> #crate_path::Result<()> {
                if #enabled {
                    #crdt::Resolution::resolve_fields(self, actor)?;
                }
                ::core::result::Result::Ok(())
            }
        }
    });
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #crdt::Record for #name #type_generics #where_clause {
            const KEY: &'static [u8] = #key;
            const FORMAT: &'static [u8; 8] = #format;
            const SIZE: usize = 0 #(+ <#types as #crdt::Record>::SIZE)*;

            fn encode(self, __weekend_bytes: &mut [u8]) -> #crate_path::Result<()> {
                if __weekend_bytes.len() != <Self as #crdt::Record>::SIZE {
                    return ::core::result::Result::Err(#crate_path::Error::Buffer);
                }
                #(
                    let (__weekend_field, __weekend_bytes) =
                        __weekend_bytes.split_at_mut(<#types as #crdt::Record>::SIZE);
                    #crdt::Record::encode(self.#names, __weekend_field)?;
                )*
                ::core::debug_assert!(__weekend_bytes.is_empty());
                ::core::result::Result::Ok(())
            }

            fn decode(__weekend_bytes: &[u8]) -> #crate_path::Result<Self> {
                if __weekend_bytes.len() != <Self as #crdt::Record>::SIZE {
                    return ::core::result::Result::Err(#crate_path::Error::Corrupt);
                }
                #(
                    let (__weekend_field, __weekend_bytes) =
                        __weekend_bytes.split_at(<#types as #crdt::Record>::SIZE);
                    let #decoded = <#types as #crdt::Record>::decode(__weekend_field)?;
                )*
                ::core::debug_assert!(__weekend_bytes.is_empty());
                ::core::result::Result::Ok(Self { #(#names: #decoded),* })
            }

            fn merge(&mut self, other: Self) {
                #(#crdt::Record::merge(&mut self.#names, other.#names);)*
            }

            fn check_peer(self, incoming: Self, local_actor: #crdt::Actor) -> #crate_path::Result<()> {
                #(#crdt::Record::check_peer(self.#names, incoming.#names, local_actor)?;)*
                ::core::result::Result::Ok(())
            }

            #resolution
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_incomplete_or_ambiguous_wire_order() {
        for (order, expected) in [
            ("a", "every field"),
            ("a, a", "duplicate field"),
            ("a, c", "unknown field"),
        ] {
            let source = format!(
                "#[record(key = b\"r\", format = b\"EXAMPLE1\", fields({order}))] struct R {{ a: A, b: B }}"
            );
            let error = expand(syn::parse_str(&source).unwrap()).expect_err("invalid wire order");
            assert!(error.to_string().contains(expected), "{error}");
        }
    }
}
