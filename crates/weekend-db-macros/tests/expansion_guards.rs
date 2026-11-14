//! Call the production expansion functions in this process so their diagnostic
//! branches have attributable coverage; downstream rustc controls remain in
//! compile_fail.rs. Source modules are imported unchanged, including their tests.
#[path = "../src/model.rs"]
mod model;
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
