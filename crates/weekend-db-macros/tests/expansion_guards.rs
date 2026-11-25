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

const RECORD_OPTIONS: &str = "key=b\"key\",format=b\"FORMAT01\",fields(a)";

fn record_source(options: &str) -> String {
    format!("#[record({options})] struct A {{ a:u64 }}")
}

#[test]
fn record_options_emit_the_requested_constants_crate_and_resolution_hook() {
    use quote::ToTokens;
    use syn::ImplItem;

    for (extra, path, hook) in [
        ("", ":: weekend_db :: crdt :: Record", false),
        (
            ",crate=\"crate::api\"",
            "crate :: api :: crdt :: Record",
            false,
        ),
        (",resolve=FLAG", ":: weekend_db :: crdt :: Record", true),
    ] {
        let source = format!(
            "#[allow(dead_code)] {}",
            record_source(&format!("{RECORD_OPTIONS}{extra}"))
        );
        let output = record::expand(syn::parse_str(&source).unwrap()).unwrap();
        let implementation: syn::ItemImpl = syn::parse2(output).unwrap();
        assert_eq!(
            implementation
                .trait_
                .unwrap()
                .1
                .to_token_stream()
                .to_string(),
            path,
        );
        let constants = implementation.items.iter().filter_map(|item| {
            let ImplItem::Const(item) = item else {
                return None;
            };
            Some((
                item.ident.to_string(),
                item.expr.to_token_stream().to_string(),
            ))
        });
        assert_eq!(
            constants.take(2).collect::<Vec<_>>(),
            [
                ("KEY".into(), "b\"key\"".into()),
                ("FORMAT".into(), "b\"FORMAT01\"".into()),
            ],
        );
        assert_eq!(
            implementation.items.iter().any(|item| {
                matches!(item, ImplItem::Fn(method) if method.sig.ident == "resolve_conflicts")
            }),
            hook,
        );
    }
    rejected(
        record::expand,
        &record_source(&format!("{RECORD_OPTIONS},unknown=true")),
        "expected key, format, fields(...), resolve, or crate",
    );
}

#[test]
fn record_rejects_each_duplicate_option() {
    valid(record::expand, &record_source(RECORD_OPTIONS));
    valid(
        record::expand,
        &record_source(&format!(
            "{RECORD_OPTIONS},crate=\"crate::api\",resolve=true"
        )),
    );
    for (extra, message) in [
        ("fields(a)", "duplicate fields list"),
        (
            "crate=\"crate::api\",crate=\"crate::api\"",
            "duplicate crate path",
        ),
        ("key=b\"key\"", "duplicate record option"),
        ("format=b\"FORMAT01\"", "duplicate record option"),
        ("resolve=true,resolve=true", "duplicate record option"),
    ] {
        rejected(
            record::expand,
            &record_source(&format!("{RECORD_OPTIONS},{extra}")),
            message,
        );
    }
    valid(
        record::expand,
        "#[record(key=b\"key\")] #[record(format=b\"FORMAT01\",fields(a))] struct A { a:u64 }",
    );
    rejected(
        record::expand,
        &format!("#[record(key=b\"key\")] {}", record_source(RECORD_OPTIONS)),
        "duplicate record option",
    );
}

#[test]
fn record_requires_each_wire_option() {
    valid(record::expand, &record_source(RECORD_OPTIONS));
    for (options, message) in [
        ("format=b\"FORMAT01\",fields(a)", "specify record key"),
        ("key=b\"key\",fields(a)", "specify record format"),
        (
            "key=b\"key\",format=b\"FORMAT01\"",
            "list fields in wire order with fields(...)",
        ),
    ] {
        rejected(record::expand, &record_source(options), message);
    }
    rejected(record::expand, "struct A { a:u64 }", "specify record key");
}

#[test]
fn record_propagates_each_option_parser_error() {
    valid(
        record::expand,
        &record_source(&format!("{RECORD_OPTIONS},crate=\"crate::api\"")),
    );
    for (options, message) in [
        (
            "key=b\"key\",format=b\"FORMAT01\",fields(3)",
            "expected identifier",
        ),
        (
            "key=b\"key\",format=b\"FORMAT01\",fields",
            "unexpected end of input, expected parentheses",
        ),
        ("key,format=b\"FORMAT01\",fields(a)", "expected `=`"),
        (
            "format=b\"FORMAT01\",fields(a),key=",
            "unexpected end of input, expected an expression",
        ),
    ] {
        rejected(record::expand, &record_source(options), message);
    }
    for (extra, message) in [
        ("crate", "expected `=`"),
        ("crate=true", "expected string literal"),
        ("crate=\"crate::api!\"", "unexpected token"),
    ] {
        rejected(
            record::expand,
            &record_source(&format!("{RECORD_OPTIONS},{extra}")),
            message,
        );
    }
}

#[test]
fn record_attribute_requires_parentheses() {
    valid(record::expand, &record_source(RECORD_OPTIONS));
    rejected(
        record::expand,
        "#[record=\"invalid\"] struct A { a:u64 }",
        "expected parentheses: #[record(...)]",
    );
}

#[test]
fn record_attribute_on_a_field_rejects_with_unrelated_attribute_control() {
    let source = |field| format!("#[record({RECORD_OPTIONS})] struct A {{ {field} a:u64 }}");
    valid(record::expand, &source("#[allow(dead_code)]"));
    rejected(
        record::expand,
        &source("#[record(key=b\"key\")]"),
        "put #[record(...)] on the struct",
    );
}

const MODEL_META: &str = "type_id=7,version=2";

fn model_source(meta: &str, fields: &str) -> String {
    format!("#[weekend({meta})] struct A {{ {fields} }}")
}

#[test]
fn model_metadata_and_fields_emit_the_selected_schema() {
    use quote::quote;

    for (crate_path, emitted_path) in [
        ("", quote!(::weekend_db_typed)),
        (",crate=\"crate::typed\"", quote!(crate::typed)),
    ] {
        let source = model_source(
            &format!("{MODEL_META},reserved(8,9){crate_path}"),
            "#[allow(dead_code)] #[weekend(id=1,unique,name=\"lookup\")] a:String,
             #[weekend(id=2,default)] r#type:u64, #[weekend(id=3,default=17)] b:u64",
        );
        let input = syn::parse_str(&format!("#[allow(dead_code)] {source}")).unwrap();
        let output = model::expand(input).unwrap().to_string();
        for expected in [
            quote!(impl #emitted_path::Model for A),
            quote!(
                const TYPE_ID: u64 = 7u64;
            ),
            quote!(
                const VERSION: u64 = 2u64;
            ),
            quote!(pub const A: #emitted_path::Index<Self, String> = #emitted_path::Index::new(1u64);),
            quote!(
                pub const TYPE: u64 = 2u64;
            ),
            quote!(
                const UNIQUE_FIELDS: &'static [u64] = &[1u64];
            ),
            quote!(name: "lookup"),
            quote!(name: "type"),
            quote!(encode_field(&(::core::default::Default::default()), db)),
            quote!(encode_field(&(17), db)),
            quote!(&[8u64, 9u64]),
        ] {
            let fragment = expected.to_string();
            assert!(output.contains(&fragment), "missing {fragment} in {output}");
        }
    }
    for (source, expected) in [
        (
            model_source(&format!("{MODEL_META},unknown"), ""),
            "expected type_id, version, reserved(...), or crate",
        ),
        (
            model_source(MODEL_META, "#[weekend(id=1,unknown)] a:u64"),
            "expected id, unique, default, or name",
        ),
    ] {
        rejected(model::expand, &source, expected);
    }
}

#[test]
fn model_rejects_each_duplicate_struct_option() {
    valid(
        model::expand,
        &model_source(
            &format!("{MODEL_META},reserved(),crate=\"crate::typed\""),
            "",
        ),
    );
    valid(
        model::expand,
        "#[weekend(type_id=7)] #[weekend(version=2)] struct A {}",
    );
    for (option, expected) in [
        ("version=2", "duplicate version"),
        ("reserved(9,9)", "duplicate reserved field ID"),
        (
            "crate=\"crate::typed\",crate=\"crate::typed\"",
            "duplicate crate path",
        ),
    ] {
        rejected(
            model::expand,
            &model_source(&format!("{MODEL_META},{option}"), ""),
            expected,
        );
    }
}

#[test]
fn model_rejects_each_duplicate_field_option() {
    let field = |options| model_source(MODEL_META, &format!("#[weekend({options})] a:u64"));
    valid(
        model::expand,
        &field("id=1,unique,default=17,name=\"stored\""),
    );
    valid(
        model::expand,
        &model_source(
            MODEL_META,
            "#[weekend(id=1)] #[weekend(name=\"stored\")] a:u64",
        ),
    );
    for (options, expected) in [
        ("id=1,id=1", "duplicate field ID"),
        ("id=1,unique,unique", "duplicate unique"),
        ("id=1,default,default=17", "duplicate default"),
        ("id=1,name=\"stored\",name=\"stored\"", "duplicate name"),
    ] {
        rejected(model::expand, &field(options), expected);
    }
}

#[test]
fn model_rejects_empty_repeated_and_colliding_field_names() {
    valid(
        model::expand,
        &model_source(
            MODEL_META,
            "#[weekend(id=1,name=\"first\")] a:u64, #[weekend(id=2,name=\"second\")] b:u64",
        ),
    );
    for (fields, expected) in [
        (
            "#[weekend(id=1,name=\"\")] a:u64",
            "field name cannot be empty",
        ),
        (
            "#[weekend(id=1,name=\"first\")] a:u64, #[weekend(id=2,name=\"first\")] b:u64",
            "duplicate stored field name",
        ),
        (
            "#[weekend(id=1,name=\"first\")] a:u64, #[weekend(id=2,name=\"second\")] A:u64",
            "generated field constant names collide",
        ),
    ] {
        rejected(model::expand, &model_source(MODEL_META, fields), expected);
    }
}

#[test]
fn model_requires_each_id_and_schema_version() {
    let fields = "#[weekend(id=1)] a:u64";
    valid(model::expand, &model_source(MODEL_META, fields));
    rejected(
        model::expand,
        "struct A {}",
        "add #[weekend(type_id = ..., version = ...)]",
    );
    rejected(
        model::expand,
        &model_source("version=2", fields),
        "add #[weekend(type_id = ..., version = ...)]",
    );
    rejected(
        model::expand,
        &model_source("type_id=7", fields),
        "missing explicit schema version",
    );
    rejected(
        model::expand,
        &model_source(MODEL_META, "a:u64"),
        "each field needs #[weekend(id = ...)]",
    );
}

#[test]
fn model_numeric_options_preserve_separator_literal_and_zero_diagnostics() {
    let fields = "#[weekend(id=1)] a:u64";
    valid(
        model::expand,
        &model_source(&format!("{MODEL_META},reserved(9)"), fields),
    );
    for (meta, fields, expected) in [
        ("version=2,type_id", fields, "expected `=`"),
        ("version=2,type_id=true", fields, "expected integer literal"),
        (
            "version=2,type_id=0",
            fields,
            "stable IDs and versions must be nonzero",
        ),
        ("type_id=7,version", fields, "expected `=`"),
        ("type_id=7,version=true", fields, "expected integer literal"),
        (
            "type_id=7,version=0",
            fields,
            "stable IDs and versions must be nonzero",
        ),
        (
            "type_id=7,version=2,reserved(true)",
            fields,
            "expected integer literal",
        ),
        (
            "type_id=7,version=2,reserved(0)",
            fields,
            "stable IDs and versions must be nonzero",
        ),
        (MODEL_META, "#[weekend(id)] a:u64", "expected `=`"),
        (
            MODEL_META,
            "#[weekend(id=true)] a:u64",
            "expected integer literal",
        ),
        (
            MODEL_META,
            "#[weekend(id=0)] a:u64",
            "stable IDs and versions must be nonzero",
        ),
    ] {
        rejected(model::expand, &model_source(meta, fields), expected);
    }
}

#[test]
fn model_numeric_conversion_reports_overflow() {
    const MAX: &str = "type_id=18446744073709551615,version=2";
    valid(model::expand, &model_source(MAX, ""));
    rejected(
        model::expand,
        &model_source("type_id=18446744073709551616,version=2", ""),
        "number too large to fit in target type",
    );
}

#[test]
fn model_crate_name_and_default_parsers_preserve_diagnostics() {
    valid(
        model::expand,
        &model_source(
            &format!("{MODEL_META},crate=\"crate::typed\""),
            "#[weekend(id=1,name=\"stored\",default=17)] a:u64",
        ),
    );
    for (option, expected) in [
        ("crate", "expected `=`"),
        ("crate=true", "expected string literal"),
        ("crate=\"crate::typed!\"", "unexpected token"),
    ] {
        rejected(
            model::expand,
            &model_source(&format!("{MODEL_META},{option}"), ""),
            expected,
        );
    }
    for (option, expected) in [
        ("name", "expected `=`"),
        ("name=true", "expected string literal"),
        (
            "default=",
            "unexpected end of input, expected an expression",
        ),
    ] {
        rejected(
            model::expand,
            &model_source(MODEL_META, &format!("#[weekend(id=1,{option})] a:u64")),
            expected,
        );
    }
}

#[test]
fn model_struct_attribute_requires_parentheses() {
    valid(model::expand, &model_source(MODEL_META, ""));
    rejected(
        model::expand,
        "#[weekend=\"invalid\"] struct A {}",
        "expected parentheses: #[weekend(...)]",
    );
}

#[test]
fn model_field_attribute_requires_parentheses() {
    valid(
        model::expand,
        &model_source(MODEL_META, "#[weekend(id=1)] a:u64"),
    );
    rejected(
        model::expand,
        &model_source(MODEL_META, "#[weekend=\"invalid\"] a:u64"),
        "expected parentheses: #[weekend(...)]",
    );
}
