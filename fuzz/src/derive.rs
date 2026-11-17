// Same implementation files used by the proc-macro entrypoints; no duplicate parser.
#[path = "../../crates/weekend-db-macros/src/model.rs"]
mod model;
#[path = "../../crates/weekend-db-macros/src/record.rs"]
mod record;
#[path = "../../crates/weekend-db-macros/src/resolution.rs"]
mod resolution;

pub fn derive_tokens(input: &[u8]) {
    if input.len() > 8192 {
        return;
    }
    let raw = String::from_utf8_lossy(input);
    let a = input.first().copied().unwrap_or(0);
    let b = input.get(1).copied().unwrap_or(0);
    let generated = format!(
        "#[weekend(type_id={a},version={b})] struct Item {{ #[weekend(id={a},unique)] a:u64, #[weekend(id={b})] b:String }}"
    );
    for source in [raw.as_ref(), generated.as_str()] {
        if let Ok(ast) = syn::parse_str::<syn::DeriveInput>(source) {
            for expand in [model::expand, record::expand, resolution::expand] {
                let output = expand(ast.clone()).unwrap_or_else(syn::Error::into_compile_error);
                // Successful expansions and diagnostics must remain syntactically valid Rust.
                syn::parse2::<syn::File>(output).expect("derive emitted invalid Rust syntax");
            }
        }
    }
}

#[test]
fn replay() {
    for source in [
        "#[record(key=b\"key\",format=b\"12345678\",fields(a))] struct A { a:u64 }",
        "struct A { #[resolve] a:u64 }",
        "#[weekend(type_id=1,version=1)] struct A { #[weekend(id=1)] a:String }",
        "#[weekend(type_id=0)] enum A { B }",
        "garbage",
    ] {
        derive_tokens(source.as_bytes());
    }
    for a in 0..16 {
        for b in 0..16 {
            derive_tokens(&[a, b]);
        }
    }
}
