use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Expr, Fields, LitInt, LitStr, Path, parse::Parse, spanned::Spanned};

fn nonzero(lit: LitInt) -> syn::Result<u64> {
    let n = lit.base10_parse()?;
    if n == 0 {
        Err(syn::Error::new(
            lit.span(),
            "stable IDs and versions must be nonzero",
        ))
    } else {
        Ok(n)
    }
}
pub(crate) fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        return Err(syn::Error::new(
            input.generics.span(),
            "Model requires an owned, nongeneric struct",
        ));
    }
    let mut type_id = None;
    let mut version = None;
    let mut retired = Vec::new();
    let mut path: Path = syn::parse_quote!(::weekend_db_typed);
    let mut crate_seen = false;
    for attr in &input.attrs {
        if attr.path().is_ident("weekend") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("type_id") {
                    if type_id.is_some() {
                        return Err(meta.error("duplicate type_id"));
                    }
                    type_id = Some(nonzero(meta.value()?.parse()?)?);
                } else if meta.path.is_ident("version") {
                    if version.is_some() {
                        return Err(meta.error("duplicate version"));
                    }
                    version = Some(nonzero(meta.value()?.parse()?)?);
                } else if meta.path.is_ident("reserved") {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    for lit in content.parse_terminated(LitInt::parse, syn::Token![,])? {
                        let id = nonzero(lit)?;
                        if retired.contains(&id) {
                            return Err(meta.error("duplicate reserved field ID"));
                        }
                        retired.push(id);
                    }
                } else if meta.path.is_ident("crate") {
                    if crate_seen {
                        return Err(meta.error("duplicate crate path"));
                    }
                    crate_seen = true;
                    path = meta.value()?.parse::<LitStr>()?.parse()?;
                } else {
                    return Err(meta.error("expected type_id, version, reserved(...), or crate"));
                }
                Ok(())
            })?;
        }
    }
    let type_id = type_id.ok_or_else(|| {
        syn::Error::new(
            input.ident.span(),
            "add #[weekend(type_id = ..., version = ...)]",
        )
    })?;
    let version = version
        .ok_or_else(|| syn::Error::new(input.ident.span(), "missing explicit schema version"))?;
    let fields = match input.data {
        Data::Struct(s) => match s.fields {
            Fields::Named(f) => f.named,
            _ => {
                return Err(syn::Error::new(
                    s.fields.span(),
                    "Model requires named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new(
                input.ident.span(),
                "Model supports structs only",
            ));
        }
    };
    let mut ids = Vec::new();
    let mut constants = Vec::new();
    let mut specs = Vec::new();
    let mut decodes = Vec::new();
    let mut patches = Vec::new();
    let mut uniques = Vec::new();
    let mut checks = Vec::new();
    let mut names = Vec::new();
    for field in fields {
        let ident = field
            .ident
            .as_ref()
            .ok_or_else(|| syn::Error::new(field.span(), "expected named field"))?;
        let ty = &field.ty;
        let mut id = None;
        let mut unique = false;
        let mut default: Option<Expr> = None;
        let mut label = ident.to_string().trim_start_matches("r#").to_string();
        let mut label_seen = false;
        for attr in &field.attrs {
            if attr.path().is_ident("weekend") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("id") {
                        if id.is_some() {
                            return Err(meta.error("duplicate field ID"));
                        }
                        id = Some(nonzero(meta.value()?.parse()?)?);
                    } else if meta.path.is_ident("unique") {
                        if unique {
                            return Err(meta.error("duplicate unique"));
                        }
                        unique = true;
                    } else if meta.path.is_ident("default") {
                        if default.is_some() {
                            return Err(meta.error("duplicate default"));
                        }
                        default = Some(if meta.input.peek(syn::Token![=]) {
                            meta.value()?.parse()?
                        } else {
                            syn::parse_quote!(::core::default::Default::default())
                        });
                    } else if meta.path.is_ident("name") {
                        if label_seen {
                            return Err(meta.error("duplicate name"));
                        }
                        label_seen = true;
                        label = meta.value()?.parse::<LitStr>()?.value();
                        if label.is_empty() {
                            return Err(meta.error("field name cannot be empty"));
                        }
                    } else {
                        return Err(meta.error("expected id, unique, default, or name"));
                    }
                    Ok(())
                })?;
            }
        }
        let id = id.ok_or_else(|| {
            syn::Error::new(ident.span(), "each field needs #[weekend(id = ...)]")
        })?;
        if ids.contains(&id) || retired.contains(&id) {
            return Err(syn::Error::new(
                ident.span(),
                "field ID duplicates an active or reserved ID",
            ));
        }
        ids.push(id);
        if names.contains(&label) {
            return Err(syn::Error::new(ident.span(), "duplicate stored field name"));
        }
        names.push(label.clone());
        let constant = format_ident!(
            "{}",
            ident.to_string().trim_start_matches("r#").to_uppercase()
        );
        if constants.iter().any(|(name, _)| name == &constant) {
            return Err(syn::Error::new(
                ident.span(),
                "generated field constant names collide",
            ));
        }
        let declaration = if unique {
            uniques.push(id);
            checks.push(quote!(#path::__private::unique::<#ty>();));
            quote!(pub const #constant: #path::Index<Self,#ty> = #path::Index::new(#id);)
        } else {
            quote!(pub const #constant: u64 = #id;)
        };
        constants.push((constant, declaration));
        let default_value = default
            .as_ref()
            .map(|expr| quote!(<#ty as #path::FieldValue>::encode_field(&(#expr), db)?))
            .unwrap_or_else(|| quote!(::core::option::Option::None));
        let default_read = default
            .as_ref()
            .map(|expr| quote!(::core::option::Option::Some(|| -> #ty {#expr})))
            .unwrap_or_else(|| quote!(::core::option::Option::None));
        checks.push(quote!(<#ty as #path::FieldValue>::validate_type()?;));
        specs.push(quote!(#path::engine::schema::FieldSpec {id:#id,name:#label,kind:<#ty as #path::FieldValue>::KIND,required:!<#ty as #path::FieldValue>::OPTIONAL,nullable:<#ty as #path::FieldValue>::NULLABLE,default:#default_value}));
        decodes.push(quote!(#ident: #path::__private::read::<#ty>(record, schema, #id, <Self as #path::Model>::VERSION, #default_read)?));
        patches.push(quote!(raw = #path::__private::set(db, raw, #id, &self.#ident)?;));
    }
    let name = input.ident;
    let constants = constants.into_iter().map(|(_, declaration)| declaration);
    Ok(quote! {
        impl #name {#(#constants)*}
        impl #path::Model for #name {
            const TYPE_ID:u64=#type_id;
            const VERSION:u64=#version;
            const UNIQUE_FIELDS:&'static [u64]=&[#(#uniques),*];
            fn schema(db: &#path::engine::Database) -> #path::Result<#path::Schema> {
                #(#checks)*
                ::core::result::Result::Ok(#path::Schema::build(db,<Self as #path::Model>::TYPE_ID,<Self as #path::Model>::VERSION,&[#(#specs),*],&[#(#retired),*])?)
            }
            fn decode(record: &#path::Record, schema: &#path::Schema) -> #path::Result<Self> {
                #path::__private::validate(record,schema,<Self as #path::Model>::TYPE_ID)?;
                ::core::result::Result::Ok(Self {#(#decodes),*})
            }
            fn patch(&self,db:&#path::engine::Database,record:&#path::Record) -> #path::Result<#path::Record> {
                if record.type_id()!=<Self as #path::Model>::TYPE_ID {return ::core::result::Result::Err(#path::Error::TypeMismatch);}
                let mut raw=::core::clone::Clone::clone(record);
                #(#patches)*
                #(raw=raw.remove(#retired)?;)*
                ::core::result::Result::Ok(raw)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_ambiguous_model_definitions_with_useful_diagnostics() {
        for (source, expected) in [
            ("struct A { a: String }", "type_id"),
            (
                "#[weekend(type_id=1,version=0)] struct A { #[weekend(id=1)] a: String }",
                "nonzero",
            ),
            (
                "#[weekend(type_id=1,version=1)] struct A { a: String }",
                "each field",
            ),
            (
                "#[weekend(type_id=1,version=1)] struct A { #[weekend(id=1)] a:String, #[weekend(id=1)] b:bool }",
                "duplicates",
            ),
            (
                "#[weekend(type_id=1,version=1,reserved(1))] struct A { #[weekend(id=1)] a:String }",
                "reserved",
            ),
            (
                "#[weekend(type_id=1,version=1)] struct A<T> { #[weekend(id=1)] a:T }",
                "nongeneric",
            ),
            (
                "#[weekend(type_id=1,version=1)] struct A(String);",
                "named fields",
            ),
            (
                "#[weekend(type_id=1,version=1)] struct A { #[weekend(id=1,uniqe)] a:String }",
                "expected id",
            ),
        ] {
            let error = expand(syn::parse_str(source).unwrap()).expect_err("expected diagnostic");
            assert!(error.to_string().contains(expected), "{error}");
        }
    }
}
