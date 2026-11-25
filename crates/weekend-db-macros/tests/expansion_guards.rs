//! Call the production expansion functions in this process so their diagnostic
//! branches have attributable coverage; downstream rustc controls remain in
//! compile_fail.rs. Source modules are imported unchanged, including their tests.
#[path = "../src/model.rs"]
mod model;
#[path = "../src/record.rs"]
mod record;
#[path = "../src/resolution.rs"]
mod resolution;

fn valid(expand: fn(syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream>, input: &str) {
    let output = expand(syn::parse_str(input).unwrap()).unwrap();
    syn::parse2::<syn::File>(output).unwrap();
}
fn rejected(
    expand: fn(syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream>,
    input: &str,
    expected: &str,
) {
    let error = expand(syn::parse_str(input).unwrap()).unwrap_err();
    assert_eq!(error.to_string(), expected);
}

#[test]
fn duplicate_model_type_id_has_single_declaration_control() {
    valid(model::expand, "#[weekend(type_id=1,version=1)] struct A {}");
    rejected(
        model::expand,
        "#[weekend(type_id=1,type_id=2,version=1)] struct A {}",
        "duplicate type_id",
    );
}

#[test]
fn resolve_on_struct_rejects_with_valid_field_control() {
    valid(resolution::expand, "struct A { #[resolve] a:u64 }");
    rejected(
        resolution::expand,
        "#[resolve] struct A { #[resolve] a:u64 }",
        "put #[resolve] on a field",
    );
}

#[test]
fn unknown_resolution_option_rejects_with_valid_crate_control() {
    valid(
        resolution::expand,
        "#[resolution(crate=\"crate::api\")] struct A { #[resolve] a:u64 }",
    );
    rejected(
        resolution::expand,
        "#[resolution(unknown=\"crate::api\")] struct A { #[resolve] a:u64 }",
        "expected crate",
    );
}

#[test]
fn duplicate_field_resolution_rejects_with_single_marker_control() {
    valid(resolution::expand, "struct A { #[resolve] a:u64 }");
    rejected(
        resolution::expand,
        "struct A { #[resolve] #[resolve] a:u64 }",
        "duplicate #[resolve]",
    );
}

#[test]
fn model_parameters_and_where_clause_reject_independently() {
    valid(model::expand, "#[weekend(type_id=1,version=1)] struct A {}");
    for input in [
        "#[weekend(type_id=1,version=1)] struct A<T> { #[weekend(id=1)] item:T }",
        "#[weekend(type_id=1,version=1)] struct A where u64: Copy {}",
    ] {
        rejected(
            model::expand,
            input,
            "Model requires an owned, nongeneric struct",
        );
    }
}

#[test]
fn active_and_reserved_field_id_collisions_reject_independently() {
    let source = |second_id| {
        format!(
            "#[weekend(type_id=1,version=1,reserved(9))] struct A {{ #[weekend(id=1)] a:u64, #[weekend(id={second_id})] b:u64 }}"
        )
    };
    valid(model::expand, &source(2));
    for id in [1, 9] {
        rejected(
            model::expand,
            &source(id),
            "field ID duplicates an active or reserved ID",
        );
    }
}

#[test]
fn record_wire_order_rejects_duplicate_unknown_and_omitted_fields() {
    let source = |order: &str| {
        format!(
            "#[record(key=b\"r\",format=b\"RECORD01\",fields({order}))] struct A {{ a:u64,b:u64 }}"
        )
    };
    valid(record::expand, &source("b,a"));
    for (order, diagnostic) in [
        ("a,a", "duplicate field in wire order"),
        ("a,c", "unknown field in wire order"),
        ("a", "list every field exactly once in fields(...)"),
    ] {
        rejected(record::expand, &source(order), diagnostic);
    }
}

#[test]
fn resolution_requires_a_selected_field() {
    valid(resolution::expand, "struct A { #[resolve] a:u64 }");
    rejected(
        resolution::expand,
        "struct A { a:u64 }",
        "mark a field with #[resolve]",
    );
}

const CUSTOM_RESOLUTION: &str = "#[resolution(crate=\"crate::api\")] struct A { #[resolve] a:u64 }";

fn resolution_calls(source: &str, api: proc_macro2::TokenStream, fields: &[&str]) {
    let input = syn::parse_str(source).unwrap();
    let actual = resolution::expand(input).unwrap();
    let fields = fields
        .iter()
        .map(|name| syn::parse_str::<syn::Ident>(name).unwrap());
    let expected = quote::quote! {
        impl #api::crdt::Resolution for A {
            fn resolve_fields(&mut self, actor: #api::crdt::Actor) -> #api::Result<()> {
                #(#api::crdt::Resolution::resolve_fields(&mut self.#fields, actor)?;)*
                ::core::result::Result::Ok(())
            }
        }
    };
    assert_eq!(actual.to_string(), expected.to_string(), "{source}");
}

#[test]
fn resolution_crate_override_changes_the_emitted_paths() {
    resolution_calls(
        "struct A { #[resolve] a:u64 }",
        quote::quote!(::weekend_db),
        &["a"],
    );
    resolution_calls(
        "#[allow(dead_code)] struct A { #[resolve] a:u64 }",
        quote::quote!(::weekend_db),
        &["a"],
    );
    resolution_calls(CUSTOM_RESOLUTION, quote::quote!(crate::api), &["a"]);
    resolution_calls(
        "#[allow(dead_code)] #[resolution(crate=\"crate::api\")] struct A { #[resolve] a:u64 }",
        quote::quote!(crate::api),
        &["a"],
    );
}

#[test]
fn resolution_rejects_repeated_crate_even_across_attributes() {
    valid(resolution::expand, CUSTOM_RESOLUTION);
    for source in [
        "#[resolution(crate=\"crate::api\",crate=\"crate::api\")] struct A { #[resolve] a:u64 }",
        "#[resolution(crate=\"crate::api\")] #[resolution(crate=\"crate::api\")] struct A { #[resolve] a:u64 }",
    ] {
        rejected(resolution::expand, source, "duplicate crate path");
    }
}

#[test]
fn resolution_crate_directive_rejects_on_a_field() {
    valid(
        resolution::expand,
        "struct A { #[resolve] #[allow(dead_code)] a:u64 }",
    );
    rejected(
        resolution::expand,
        "struct A { #[resolve] #[resolution(crate=\"crate::api\")] a:u64 }",
        "put #[resolution(crate = ...)] on the struct",
    );
}

#[test]
fn resolution_fields_follow_marker_selection() {
    let default_crate = quote::quote!(::weekend_db);
    resolution_calls(
        "struct A { #[resolve] a:u64, #[doc=\"second\"] b:u64, c:u64 }",
        default_crate.clone(),
        &["a"],
    );
    resolution_calls(
        "struct A { #[resolve] a:u64, #[doc=\"second\"] #[resolve] b:u64, c:u64 }",
        default_crate,
        &["a", "b"],
    );
}

#[test]
fn resolution_rejects_non_bare_markers() {
    valid(resolution::expand, "struct A { #[resolve] a:u64 }");
    for source in [
        "struct A { #[resolve()] a:u64 }",
        "struct A { #[resolve=true] a:u64 }",
    ] {
        rejected(resolution::expand, source, "expected #[resolve]");
    }
}

#[test]
fn resolution_crate_value_requires_equals() {
    valid(resolution::expand, CUSTOM_RESOLUTION);
    rejected(
        resolution::expand,
        "#[resolution(crate)] struct A { #[resolve] a:u64 }",
        "expected `=`",
    );
}

#[test]
fn resolution_crate_value_requires_string_literal() {
    valid(resolution::expand, CUSTOM_RESOLUTION);
    rejected(
        resolution::expand,
        "#[resolution(crate=true)] struct A { #[resolve] a:u64 }",
        "expected string literal",
    );
}

#[test]
fn resolution_crate_literal_requires_valid_path() {
    valid(resolution::expand, CUSTOM_RESOLUTION);
    rejected(
        resolution::expand,
        "#[resolution(crate=\"crate::api!\")] struct A { #[resolve] a:u64 }",
        "unexpected token",
    );
}

#[test]
fn resolution_attribute_requires_parentheses() {
    valid(resolution::expand, CUSTOM_RESOLUTION);
    rejected(
        resolution::expand,
        "#[resolution=\"crate::api\"] struct A { #[resolve] a:u64 }",
        "expected parentheses: #[resolution(...)]",
    );
}
