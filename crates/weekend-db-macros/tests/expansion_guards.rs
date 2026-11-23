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
