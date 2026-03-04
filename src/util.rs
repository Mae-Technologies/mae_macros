use quote::quote;
use syn::{Data, DataStruct, DeriveInput, Field, Fields, LitStr};

type Body = proc_macro2::TokenStream;
type BodyIdent = proc_macro2::TokenStream;

// TODO:
// Impl EnumIter for Fields -> this is to generate randomness for tests
// If there is a flag #[test] at the top of the repo struct to impl a randomness generator
// From impl:
// impl Patch -> Field
// impl Patch => Filter
// impl Row => Filter

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

    fields.iter().for_each(|f| {
        let name_ident = f.ident.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(&ast.ident, "missing a name field (missing ident.)",)
                .to_compile_error()
        },);

        // we need to check if either there are no attrs, or if attr != locked | != insert_only
        if let Ok(name_ident,) = name_ident
            && f.attrs
                .iter()
                .map(|a| !a.path().is_ident("locked",) && !a.path().is_ident("insert_only",),)
                .all(|a| a == true,)
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
    };
    (body, body_ident,)
}

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
    }

    let all_cols_str = all_cols.join(", ",);

    let body = quote! {
        #[allow(non_snake_case, non_camel_case_types, nonstandard_style)]
        #[derive(Clone)]
        pub enum #body_ident {
            All,
            #(#variants,)*
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
    let is_update_row = !is_insert_row;

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

    fields.iter().for_each(|f| {
        let name_ident = f.ident.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(&ast.ident, "missing a name field (missing ident.)",)
                .to_compile_error()
        },);

        // we need to check if either there are no attrs, or if attr != locked | != insert_only
        if let Ok(name_ident,) = name_ident
            && f.attrs
                .iter()
                .map(|a| {
                    attr_black_list.iter().map(|abl| !a.path().is_ident(abl,),).all(|a| a == true,)
                },)
                .all(|a| a == true,)
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
                },)
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
    };
    (body, body_ident,)
}

// Utils to find various attributes
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
