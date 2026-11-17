//! Derives for typed models and CRDT records.
use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

mod model;
mod record;
mod resolution;

/// Implements `crdt::Record` for a named struct of fixed-width `Record` fields.
/// Specify the row `key`, eight-byte `format`, and every field in wire order
/// with `#[record(key = ..., format = ..., fields(...))]`. The payload
/// concatenates the field codecs without padding or per-field headers.
/// An optional `resolve = ...` boolean enables the record's `Resolution` hook.
/// Use `crate = "path"` for a renamed `weekend_db` dependency.
#[proc_macro_derive(Record, attributes(record))]
pub fn record(input: TokenStream) -> TokenStream {
    record::expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Resolution, attributes(resolve, resolution))]
pub fn resolution(input: TokenStream) -> TokenStream {
    resolution::expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(Model, attributes(weekend))]
pub fn model(input: TokenStream) -> TokenStream {
    model::expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
