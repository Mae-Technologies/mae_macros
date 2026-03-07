//! Internal helper utilities for the `MaeRepo` derive macro.
//!
//! These functions inspect a [`DeriveInput`] struct's named fields and emit
//! the `proc_macro2::TokenStream` bodies for `InsertRow`, `UpdateRow`,
//! `Field`, and `PatchField` respectively.  They are `pub(crate)` and are
//! not part of the public API.

use quote::quote;
use syn::{Data, DataStruct, DeriveInput, Field, Fields, LitStr};

type Body = proc_macro2::TokenStream;
type BodyIdent = proc_macro2::TokenStream;

/// Generates the `PatchField` typed enum and its trait implementations.
///
/// `PatchField` is an enum where each variant carries the field's value type.
/// Fields marked `#[locked]` or `#[insert_only]` are excluded (they cannot be
/// patched).
///
/// # Returns
///
/// `(body, ident_tokens)` where `body` is the full `TokenStream` defining the
/// `PatchField` enum and its `impl` blocks, and `ident_tokens` is always
/// `quote! { PatchField }`.
#[doc(hidden)]
pub fn to_patches(ast: &DeriveInput,) -> (Body, BodyIdent,) {
    let fields = match &ast.data {
        Data::Struct(DataStruct { fields: Fields::Named(fields,), .. },) => &fields.named,
        _ => {
            return (
                syn::Error::new_spanned(&ast.ident, "expected a struct with named fields",)
                    .to_compile_error(),
                quote! { PatchField },
            );
        }
    };

    let mut to_arg = vec![];
    let mut to_string = vec![];
    let mut typed_enum = vec![];
    let body_ident = quote! { PatchField };
    let mut debug_bindings = vec![];
    let mut patch_to_field_arms = vec![];
    let mut patch_to_filter_arms = vec![];

    fields.iter().for_each(|f| {
        let name_ident = f.ident.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(&ast.ident, "missing a name field (missing ident.)",)
                .to_compile_error()
        },);

        // we need to check if either there are no attrs, or if attr != locked | != insert_only
        if let Ok(name_ident,) = name_ident
            && f.attrs
                .iter()
                .all(|a| !a.path().is_ident("locked",) && !a.path().is_ident("insert_only",),)
        {
            let ty = &f.ty;
            let name_str = name_ident.to_string();

            to_arg.push(quote! {
                #body_ident::#name_ident(arg) => args.add(arg)
            },);
            to_string.push(quote! {
                #body_ident::#name_ident(_) => #name_str.to_string()
            },);

            debug_bindings.push(quote! {
                #body_ident::#name_ident(b) => write!(f, "{:?}", b)
            },);

            typed_enum.push(quote! { #name_ident(#ty) },);

            patch_to_field_arms.push(quote! {
                #body_ident::#name_ident(_) => Field::#name_ident
            },);

            patch_to_filter_arms.push(quote! {
                #body_ident::#name_ident(v) => mae::repo::filter::FilterOp::Begin(
                    Field::#name_ident,
                    v.into_mae_filter(),
                )
            },);
        }
    },);

    let body = quote! {
        #[allow(non_snake_case, non_camel_case_types, nonstandard_style)]
        #[derive(Clone)]
        pub enum #body_ident {
            #(#typed_enum,)*
        }

        impl std::fmt::Display for #body_ident {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", match self {
                    #(#to_string,)*
                })
            }
        }

        impl mae::repo::__private__::ToSqlParts for #body_ident {
            fn to_sql_parts(&self) -> mae::repo::__private__::AsSqlParts {
                // NOTE: cannot accurately get the bind_idx. Catch it at a higher level
                (vec![self.to_string()], None)

            }
        }

        impl mae::repo::__private__::BindArgs for #body_ident {
            fn bind(&self, mut args: &mut sqlx::postgres::PgArguments) {
                use sqlx::Arguments;
                let _ = match self {
                    #(#to_arg,)*
                };
            }
            fn bind_len(&self) -> usize {
                // NOTE: There will always be one arg for a PatchField
                1
            }
        }

        impl std::fmt::Debug for #body_ident {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match self {
                    #(#debug_bindings,)*
                }
            }
        }

        /// Convert a [`PatchField`] variant into its corresponding [`Field`],
        /// discarding the contained value.
        impl From<#body_ident> for Field {
            fn from(patch: #body_ident) -> Field {
                match patch {
                    #(#patch_to_field_arms,)*
                }
            }
        }

        /// Convert a [`PatchField`] variant into a [`FilterOp<Field>`] using an
        /// equality condition (`Begin … Equals / StringIs`).
        ///
        /// Relies on [`mae::repo::__private__::IntoMaeFilter`] which is
        /// implemented for the primitive types (`i32`, `String`, and their
        /// `Option` wrappers) that appear in schema field definitions.
        impl From<#body_ident> for mae::repo::filter::FilterOp<Field> {
            fn from(patch: #body_ident) -> mae::repo::filter::FilterOp<Field> {
                use mae::repo::__private__::IntoMaeFilter;
                match patch {
                    #(#patch_to_filter_arms,)*
                }
            }
        }
    };
    (body, body_ident,)
}

/// Generates the `Field` column-name enum and its trait implementations.
///
/// `Field` has one variant per named field in the struct, plus an `All` variant
/// whose `Display` impl emits a comma-separated list of all column names
/// (suitable for `SELECT <Field::All> FROM …`).
///
/// Fields marked `#[locked]` or any other attribute are still included in
/// `Field` — it covers the full column surface including read-only columns.
///
/// # Returns
///
/// `(body, ident_tokens)` where `body` is the full `TokenStream` defining the
/// `Field` enum, `Field::iter()`, and its `impl` blocks, and `ident_tokens` is
/// always `quote! { Field }`.
#[doc(hidden)]
pub fn to_fields(ast: &DeriveInput,) -> (Body, BodyIdent,) {
    let fields = match &ast.data {
        Data::Struct(DataStruct { fields: Fields::Named(fields,), .. },) => &fields.named,
        _ => {
            return (
                syn::Error::new_spanned(&ast.ident, "expected a struct with named fields",)
                    .to_compile_error(),
                quote! { Field },
            );
        }
    };

    let mut all_cols: Vec<String,> = Vec::new();
    let mut to_string_arms: Vec<proc_macro2::TokenStream,> = Vec::new();
    let mut variants: Vec<proc_macro2::TokenStream,> = Vec::new();
    let mut iter_variants: Vec<proc_macro2::TokenStream,> = Vec::new();

    let body_ident = quote! { Field };

    for f in fields.iter() {
        let Some(name,) = f.ident.as_ref() else {
            variants.push(
                syn::Error::new_spanned(f, "expected a named field (missing ident)",)
                    .to_compile_error(),
            );
            continue;
        };

        let name_str = name.to_string();

        all_cols.push(name_str.clone(),);

        to_string_arms.push(quote! {
            #body_ident::#name => #name_str.to_string()
        },);

        variants.push(quote! { #name },);
        iter_variants.push(quote! { #body_ident::#name },);
    }

    let all_cols_str = all_cols.join(", ",);

    let body = quote! {
        #[allow(non_snake_case, non_camel_case_types, nonstandard_style)]
        #[derive(Clone)]
        pub enum #body_ident {
            All,
            #(#variants,)*
        }

        impl #body_ident {
            /// Returns an iterator over every concrete [`Field`] variant
            /// (excludes [`Field::All`]).
            ///
            /// Useful for test-data generation and introspection of the
            /// full column set.
            pub fn iter() -> impl Iterator<Item = #body_ident> {
                [#(#iter_variants,)*].into_iter()
            }
        }

        impl mae::repo::__private__::ToSqlParts for #body_ident {
            fn to_sql_parts(&self) -> mae::repo::__private__::AsSqlParts {
                (vec![self.to_string()], None)
            }
        }

        impl std::fmt::Display for #body_ident {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", match self {
                    Self::All => #all_cols_str.into(),
                    #(#to_string_arms,)*
                })
            }
        }
    };

    (body, body_ident,)
}

/// Generates either `InsertRow` or `UpdateRow` and their trait implementations.
///
/// Which row type is produced depends on `attr_black_list`:
///
/// - Pass `["locked", "update_only"]` to produce **`InsertRow`** — a plain
///   struct whose fields map 1-to-1 to the writable insert columns.
/// - Pass `["locked", "insert_only"]` to produce **`UpdateRow`** — a struct
///   where each field is `Option<T>`; only `Some` variants contribute SQL
///   `SET` clauses and bound arguments.
///
/// Both generated types implement `ToSqlParts`, `BindArgs`, `Debug`, and
/// `From<RowType> for Vec<FilterOp<Field>>`.
///
/// # Arguments
///
/// - `ast` — the parsed struct `DeriveInput`.
/// - `attr_black_list` — list of field-attribute names to skip (as `String`).
///
/// # Returns
///
/// `(body, ident_tokens)` where `body` is the full generated `TokenStream` and
/// `ident_tokens` is either `quote! { InsertRow }` or `quote! { UpdateRow }`.
#[doc(hidden)]
pub fn to_row(ast: &DeriveInput, attr_black_list: Vec<String,>,) -> (Body, BodyIdent,) {
    let fields = match &ast.data {
        Data::Struct(DataStruct { fields: Fields::Named(fields,), .. },) => &fields.named,
        _ => {
            return (
                syn::Error::new_spanned(&ast.ident, "expected a struct with named fields",)
                    .to_compile_error(),
                quote! { Row },
            );
        }
    };

    let is_insert_row = attr_black_list.contains(&"update_only".to_string(),);
    let _is_update_row = !is_insert_row;

    let body_ident = if is_insert_row {
        quote! { InsertRow}
    } else {
        quote! {UpdateRow}
    };

    let mut props = vec![];
    let mut string_some = vec![];
    let mut bind_some = vec![];
    let mut bind_len = vec![];
    let mut debug_bindings = vec![];
    let mut row_to_filter_arms = vec![];

    fields.iter().for_each(|f| {
        let name_ident = f.ident.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(&ast.ident, "missing a name field (missing ident.)",)
                .to_compile_error()
        },);

        // we need to check if either there are no attrs, or if attr != locked | != insert_only
        if let Ok(name_ident,) = name_ident
            && f.attrs.iter().all(|a| attr_black_list.iter().all(|abl| !a.path().is_ident(abl,),),)
        {
            let ty = &f.ty;
            if is_insert_row {
                props.push(quote! { pub #name_ident: #ty },);

                let name_str = name_ident.to_string();
                string_some.push(quote! {
                    i += 1;
                    sql.push(format!("{}", #name_str));
                    sql_i.push(format!("${}", i));
                },);

                bind_len.push(quote! {
                        count += 1;
                },);
                bind_some.push(quote! {
                    let _ = args.add(&self.#name_ident);
                },);
                debug_bindings.push(quote! {
                    sql_i += 1;
                    write!(f, "\n\t${} = {:?}", sql_i, &self.#name_ident)?;
                },);

                row_to_filter_arms.push(quote! {
                    {
                        use mae::repo::__private__::IntoMaeFilter;
                        let filter = row.#name_ident.clone().into_mae_filter();
                        if out.is_empty() {
                            out.push(mae::repo::filter::FilterOp::Begin(Field::#name_ident, filter,),);
                        } else {
                            out.push(mae::repo::filter::FilterOp::And(Field::#name_ident, filter,),);
                        }
                    }
                },);
            } else {
                props.push(quote! { pub #name_ident: Option<#ty> },);

                let name_str = name_ident.to_string();
                string_some.push(quote! {
                if let Some(v) = &self.#name_ident {
                    i += 1;
                    sql.push(format!("{}", #name_str));
                    sql_i.push(format!("${}", i));
                };},);

                bind_len.push(quote! {
                    if let Some(v) = &self.#name_ident {
                        count += 1;
                    };
                },);
                bind_some.push(quote! {
                if let Some(v) = &self.#name_ident {
                    let _ = args.add(v);
                };},);
                debug_bindings.push(quote! {
                    if let Some(v) = &self.#name_ident {
                        sql_i += 1;
                        write!(f, "\n\t${} = {:?}", sql_i, v)?;
                    };
                },);

                row_to_filter_arms.push(quote! {
                    if let Some(v) = row.#name_ident.clone() {
                        use mae::repo::__private__::IntoMaeFilter;
                        let filter = v.into_mae_filter();
                        if out.is_empty() {
                            out.push(mae::repo::filter::FilterOp::Begin(Field::#name_ident, filter,),);
                        } else {
                            out.push(mae::repo::filter::FilterOp::And(Field::#name_ident, filter,),);
                        }
                    }
                },);
            }
        }
    },);

    let body = quote! {
        #[allow(non_snake_case, non_camel_case_types, nonstandard_style)]
        #[derive(Clone)]
        pub struct #body_ident {
            #(#props,)*
        }

        impl mae::repo::__private__::ToSqlParts for #body_ident {
            fn to_sql_parts(&self) -> mae::repo::__private__::AsSqlParts {
                let mut i = 0;
                let mut sql = vec![];
                let mut sql_i = vec![];
                #(#string_some)*

                (sql, Some(sql_i))
            }
        }

        impl mae::repo::__private__::BindArgs for #body_ident {
            fn bind(&self, mut args: &mut sqlx::postgres::PgArguments) {
                use sqlx::Arguments;
                #(#bind_some)*
            }
            fn bind_len(&self) -> usize {
                let mut count = 0;
                #(#bind_len)*
                count
            }
        }

        impl std::fmt::Debug for #body_ident {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let mut sql_i = 0;
                #(#debug_bindings)*
                std::fmt::Result::Ok(())
            }
        }

        /// Convert a row into a list of [`FilterOp<Field>`] conditions.
        ///
        /// For [`InsertRow`] every field becomes a filter condition.
        /// For [`UpdateRow`] only the `Some` fields are emitted; `None`
        /// fields are skipped so callers can build partial WHERE clauses.
        ///
        /// The first condition uses [`FilterOp::Begin`]; subsequent ones
        /// use [`FilterOp::And`].
        impl From<#body_ident> for Vec<mae::repo::filter::FilterOp<Field>> {
            fn from(row: #body_ident) -> Vec<mae::repo::filter::FilterOp<Field>> {
                let mut out: Vec<mae::repo::filter::FilterOp<Field>> = vec![];
                #(#row_to_filter_arms)*
                out
            }
        }
    };
    (body, body_ident,)
}

// ── Attribute-search utilities ────────────────────────────────────────────────

/// Searches a field's attributes for one matching `attr_name`.
///
/// Returns the field's identifier if the attribute is present, or `None`
/// if either the field is unnamed (tuple field) or the attribute is absent.
///
/// Only the attribute path is checked; no arguments are parsed.
#[doc(hidden)]
#[allow(dead_code)]
fn find_get_attr(field: &Field, attr_name: &'static str,) -> Option<syn::Ident,> {
    let Some(ident,) = field.ident.clone() else {
        return None; // ignore tuple fields
    };

    for attr in &field.attrs {
        if attr.path().is_ident(attr_name,) {
            return Some(ident,);
        }
    }

    None
}

/// Searches a field's attributes for one matching `attr_name` and parses its
/// single string-literal argument.
///
/// Returns `Ok(Some((ident, value)))` if the attribute is found and its
/// argument is a valid string literal, `Ok(None)` if the attribute is absent
/// or the field is unnamed, and `Err` if the argument fails to parse as a
/// `LitStr`.
///
/// Expected attribute form: `#[attr_name("literal value")]`.
#[doc(hidden)]
#[allow(dead_code)]
fn find_get_attr_with_args(
    field: &Field,
    attr_name: &'static str,
) -> Result<Option<(syn::Ident, String,),>, syn::Error,> {
    let Some(ident,) = field.ident.clone() else {
        return Ok(None,); // ignore tuple fields
    };

    for attr in &field.attrs {
        if attr.path().is_ident(attr_name,) {
            let lit: LitStr = attr.parse_args().map_err(|_| {
                syn::Error::new_spanned(attr, format!("expected #[{}(\"...\")]", attr_name),)
            },)?;
            return Ok(Some((ident, lit.value(),),),);
        }
    }

    Ok(None,)
}
