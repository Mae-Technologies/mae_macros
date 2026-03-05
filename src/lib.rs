#![deny(clippy::disallowed_methods)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(non_camel_case_types, nonstandard_style)]
extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Data::Struct,
    DataStruct, DeriveInput, Fields,
    Fields::Named,
    FieldsNamed, Ident, ItemFn, LitStr, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

mod util;
use util::*;

#[proc_macro_attribute]
pub fn run_app(_: TokenStream, input: TokenStream,) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);

    // Avoid indexing panic if the function body is empty.
    let fn_block = match input_fn.block.stmts.first() {
        Some(stmt,) => stmt,
        None => {
            return syn::Error::new_spanned(
                &input_fn.sig.ident,
                "run_app requires at least one statement in the function body",
            )
            .to_compile_error()
            .into();
        }
    };

    quote! {
    async fn run<Context: Clone + Send + 'static>(
        listener: TcpListener,
        db_pool: PgPool,
        base_url: String,
        hmac_secret: SecretString,
        redis_uri: SecretString,
        custom_context: Context,
    ) -> Result<Server, anyhow::Error> {

         let redis_store = app::redis_session(redis_uri).await?;
         let server = HttpServer::new(move || {
             ActixWebApp::new()
                 .wrap(TracingLogger::default())
                 .wrap(app::session_middleware(
                     hmac_secret.clone(),
                     redis_store.clone(),
                 ))
                 .app_data(web::Data::new(ApplicationBaseUrl(base_url.clone())))
                 .app_data(web::Data::new(HmacSecret(hmac_secret.clone())))
                 .app_data(web::Data::new(db_pool.clone()))
                 .app_data(web::Data::new(custom_context.clone()))
             .#fn_block
         })
         .listen(listener)?
         .run();
         Ok(server)
         }
         }
    .into()
}

struct Args {
    ctx: Ident,
    schema: LitStr,
    _comma: Token![,],
}

impl Parse for Args {
    fn parse(input: ParseStream<'_,>,) -> syn::Result<Self,> {
        Ok(Self { ctx: input.parse()?, _comma: input.parse()?, schema: input.parse()?, },)
    }
}

#[proc_macro_attribute]
pub fn schema(args: TokenStream, input: TokenStream,) -> TokenStream {
    let Args { ctx, schema, .. } = parse_macro_input!(args as Args);
    let ast = parse_macro_input!(input as DeriveInput);

    let repo_ident = &ast.ident;
    let repo_attrs = &ast.attrs;

    // confirm the macro is being called on a Struct Type and extract the fields.
    let fields = match ast.data {
        Struct(DataStruct { fields: Named(FieldsNamed { ref named, .. },), .. },) => named,
        _ => {
            return syn::Error::new_spanned(
                repo_ident,
                "schema only works for structs with named fields",
            )
            .to_compile_error()
            .into();
        }
    };

    // rebuild the struct fields
    let params = fields.iter().map(|f| {
        let name = &f.ident;
        let ty = &f.ty;
        let attrs = &f.attrs;
        quote! {
            #(#attrs)*
            pub #name: #ty
        }
    },);

    // rebuild repo struct with the existing fields and default fields for the repo
    // NOTE: here, we are deriving the Repo with the proc_macro_derive fn from above
    let repo = quote! {

        #(#repo_attrs)*
        #[derive(mae_macros::MaeRepo, Debug, sqlx::FromRow, serde::Serialize, serde::Deserialize, Clone)]
        pub struct #repo_ident {
            #[locked]
            pub id: i32,
            #[insert_only]
            pub sys_client: i32,
            pub status: mae::repo::default::DomainStatus,
            #(#params,)*
            pub comment: Option<String>,
            #[sqlx(json)]
            pub tags: serde_json::Value,
            #[sqlx(json)]
            pub sys_detail: serde_json::Value,
            #[locked]
            pub created_by: i32,
            #[locked]
            pub updated_by: i32,
            #[locked]
            pub created_at: chrono::DateTime<chrono::Utc>,
            #[locked]
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }
        impl mae::repo::__private__::Build<#ctx, InsertRow, UpdateRow, Field, PatchField> for #repo_ident {
            fn schema() -> String {
                #schema.to_string()
            }
        }
    };
    repo.into()
}

/// Like `#[schema]` but omits the auto-injected `sys_client` field.
/// Use this for the `sys_client` table itself, which has no FK back to itself.
#[proc_macro_attribute]
pub fn schema_root(args: TokenStream, input: TokenStream,) -> TokenStream {
    let Args { ctx, schema, .. } = parse_macro_input!(args as Args);
    let ast = parse_macro_input!(input as DeriveInput);

    let repo_ident = &ast.ident;
    let repo_attrs = &ast.attrs;

    let fields = match ast.data {
        Struct(DataStruct { fields: Named(FieldsNamed { ref named, .. },), .. },) => named,
        _ => {
            return syn::Error::new_spanned(
                repo_ident,
                "schema_root only works for structs with named fields",
            )
            .to_compile_error()
            .into();
        }
    };

    let params = fields.iter().map(|f| {
        let name = &f.ident;
        let ty = &f.ty;
        let attrs = &f.attrs;
        quote! {
            #(#attrs)*
            pub #name: #ty
        }
    },);

    let repo = quote! {
        #(#repo_attrs)*
        #[derive(mae_macros::MaeRepo, Debug, sqlx::FromRow, serde::Serialize, serde::Deserialize, Clone)]
        pub struct #repo_ident {
            #[locked]
            pub id: i32,
            pub status: mae::repo::default::DomainStatus,
            #(#params,)*
            pub comment: Option<String>,
            #[sqlx(json)]
            pub tags: serde_json::Value,
            #[sqlx(json)]
            pub sys_detail: serde_json::Value,
            #[locked]
            pub created_by: i32,
            #[locked]
            pub updated_by: i32,
            #[locked]
            pub created_at: chrono::DateTime<chrono::Utc>,
            #[locked]
            pub updated_at: chrono::DateTime<chrono::Utc>,
        }
        impl mae::repo::__private__::Build<#ctx, InsertRow, UpdateRow, Field, PatchField> for #repo_ident {
            fn schema() -> String {
                #schema.to_string()
            }
        }
    };
    repo.into()
}

#[proc_macro_derive(MaeRepo, attributes(from_context, insert_only, update_only, locked))]
pub fn derive_mae_repo(item: TokenStream,) -> TokenStream {
    let ast = parse_macro_input!(item as DeriveInput);

    // Making sure it the derive macro is called on a struct;
    let _ = match &ast.data {
        Struct(DataStruct { fields: Fields::Named(fields,), .. },) => &fields.named,
        _ => {
            return syn::Error::new_spanned(
                &ast.ident,
                "MaeRepo derive expects a struct with named fields",
            )
            .to_compile_error()
            .into();
        }
    };

    let (insert_row, _,) = to_row(&ast, vec!["locked".into(), "update_only".into()],);
    let (update_row, _,) = to_row(&ast, vec!["locked".into(), "insert_only".into()],);
    let (repo_typed, _,) = to_patches(&ast,);
    let (repo_variant, _,) = to_fields(&ast,);

    quote! {
        #repo_variant
        #insert_row
        #update_row
        #repo_typed
    }
    .into()
}

// ── #[mae_test] attribute arguments ──────────────────────────────────────────

/// Parsed arguments for `#[mae_test(...)]`.
///
/// Supported forms:
/// - `#[mae_test]`                              — basic async test
/// - `#[mae_test(docker)]`                      — skip unless `MAE_TESTCONTAINERS=1` at compile time
/// - `#[mae_test(teardown = path::to::fn)]`     — call async teardown fn after the test body
/// - `#[mae_test(docker, teardown = path)]`     — both
struct MaeTestArgs {
    docker: bool,
    teardown: Option<syn::ExprPath,>,
}

impl Parse for MaeTestArgs {
    fn parse(input: ParseStream<'_,>,) -> syn::Result<Self,> {
        let mut docker = false;
        let mut teardown = None;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            match ident.to_string().as_str() {
                "docker" => docker = true,
                "teardown" => {
                    input.parse::<Token![=]>()?;
                    teardown = Some(input.parse::<syn::ExprPath>()?,);
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        format!(
                            "unknown #[mae_test] argument: `{other}`; expected `docker` or `teardown = <path>`"
                        ),
                    ),);
                }
            }
            if input.peek(Token![,],) {
                let _: Token![,] = input.parse()?;
            }
        }

        Ok(Self { docker, teardown, },)
    }
}

/// `#[mae_test]` — the standard macro for async journey tests in Mae services.
///
/// # What it does
///
/// Wraps an `async fn` test in a dedicated multi-threaded Tokio runtime, enforces the
/// Mae test-hygiene rules (no raw `.unwrap()` / `.expect()` / `assert*!` in test bodies),
/// and optionally:
/// - gates the test on `MAE_TESTCONTAINERS=1` at compile time (`docker` flag)
/// - runs an async teardown function after the test body (`teardown = path` argument)
///
/// # Attributes
///
/// | Argument | Effect |
/// |---|---|
/// | _(none)_ | Basic multi-threaded async test |
/// | `docker` | Skips test unless compiled with `MAE_TESTCONTAINERS=1` |
/// | `teardown = crate::common::context::teardown` | Calls given async fn after test, even on panic |
///
/// # Examples
///
/// ```rust,ignore
/// use mae_macros::mae_test;
///
/// // Good: returns Result, uses `?` and `must::*` helpers
/// #[mae_test]
/// async fn journey_create_user() -> Result<(), anyhow::Error> {
///     let ctx = TestContext::new().await?;
///     let user = ctx.create_user("alice").await?;
///     must_eq(user.name, "alice");
///     Ok(())
/// }
///
/// // Docker-gated: only runs when MAE_TESTCONTAINERS=1 cargo test
/// #[mae_test(docker)]
/// async fn journey_with_postgres() -> Result<(), anyhow::Error> {
///     let ctx = TestContext::new().await?;
///     // ... test using real DB
///     Ok(())
/// }
///
/// // With explicit teardown
/// #[mae_test(teardown = crate::common::context::teardown)]
/// async fn journey_with_cleanup() -> Result<(), anyhow::Error> {
///     let ctx = TestContext::setup().await?;
///     ctx.do_work().await?;
///     Ok(())
/// }
///
/// // Bad: uses raw .unwrap() — compile error
/// #[mae_test]
/// async fn bad_test() {
///     let x: Option<i32> = None;
///     let _ = x.unwrap(); // ❌ compile error: forbidden
/// }
/// ```
#[proc_macro_attribute]
pub fn mae_test(attr: TokenStream, item: TokenStream,) -> TokenStream {
    let MaeTestArgs { docker, teardown, } = parse_macro_input!(attr as MaeTestArgs);

    let mut f = match syn::parse::<syn::ItemFn,>(item,) {
        Ok(f,) => f,
        Err(_,) => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[mae_test] can only be applied to a function",
            )
            .to_compile_error()
            .into();
        }
    };

    // Tests can't take arguments.
    if !f.sig.inputs.is_empty() {
        return syn::Error::new_spanned(
            &f.sig.inputs,
            "#[mae_test] test functions must not take arguments",
        )
        .to_compile_error()
        .into();
    }

    // Capture original body before rewriting.
    let orig_block = *f.block;

    // ---- Enforce: no assert*/unwrap/expect in the user's test body ----
    // (String-based scan; simple and effective for policy enforcement.)
    let body_s = quote::quote!(#orig_block).to_string();

    let forbidden = [
        ".expect",    // Result::expect / Option::expect
        ".unwrap",    // Result::unwrap / Option::unwrap
        "assert!",    // assert!
        "assert_eq!", // assert_eq!
        "assert_ne!", // assert_ne!
    ];

    if forbidden.iter().any(|pat| body_s.contains(pat,),) {
        return syn::Error::new_spanned(
            &orig_block,
            "#[mae_test] forbids assert*/unwrap/expect in test bodies; use must::* helpers or return Result and use `?`",
        )
        .to_compile_error()
        .into();
    }

    // Extract return type as a Type.
    let ret_ty: syn::Type = match &f.sig.output {
        syn::ReturnType::Default => syn::parse_quote!(()),
        syn::ReturnType::Type(_, ty,) => (**ty).clone(),
    };

    // ---- docker gate: skip unless MAE_TESTCONTAINERS=1 was set at compile time ----
    // Uses `option_env!` so the check is baked in at compile time; no runtime overhead.
    let docker_gate = if docker {
        // Generate early-return based on whether the function returns () or a Result/other type.
        let early_return: proc_macro2::TokenStream = match &f.sig.output {
            syn::ReturnType::Default => quote! { return; },
            syn::ReturnType::Type(..,) => {
                // For Result<(), E> (the common case) this expands to Ok(()).
                // Requires the success type to implement Default; document this constraint.
                quote! { return ::core::result::Result::Ok(::core::default::Default::default()); }
            }
        };
        quote! {
            if ::std::option_env!("MAE_TESTCONTAINERS") != ::core::option::Option::Some("1") {
                // docker-gated test — recompile with `MAE_TESTCONTAINERS=1 cargo test` to run
                #early_return
            }
        }
    } else {
        quote! {}
    };

    // ---- optional teardown call ----
    let teardown_call = match teardown {
        Some(ref td_path,) => quote! {
            let __teardown_result = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                __mae_rt.block_on(async move {
                    #td_path().await;
                })
            }));
        },
        None => quote! {
            let __teardown_result: ::std::result::Result<(), Box<dyn ::std::any::Any + Send>> = Ok(());
        },
    };

    // Ensure the outer test function is synchronous; we drive an async block ourselves.
    f.sig.asyncness = None;

    // Outer test function gets ONLY #[test] (plus any attrs the user already had, e.g. #[ignore]).
    f.attrs.insert(0, syn::parse_quote!(#[test]),);

    // Generate body.
    //
    // The inner `__mae_run_test` carries `#[allow(clippy::disallowed_methods,
    // clippy::expect_used)]` because it builds the Tokio runtime via the builder API which
    // requires `.build()` — a fallible operation we must handle; we do so with a match rather
    // than `.expect()`, but the allow covers any edge cases in generated code.
    *f.block = syn::parse_quote!({
        #[allow(clippy::disallowed_methods, clippy::expect_used)]
        fn __mae_run_test() -> #ret_ty {
            #docker_gate

            let __mae_rt = match ::tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt,) => rt,
                Err(e,) => panic!("failed to build tokio runtime for #[mae_test]: {e}"),
            };

            let __user_result = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                __mae_rt.block_on(async move {
                    // run user test body
                    (async move #orig_block).await
                })
            }));

            // Always attempt teardown, even if the user body panicked.
            #teardown_call

            match (__user_result, __teardown_result) {
                (Ok(__ret), Ok(())) => __ret,

                // User panicked; teardown succeeded -> rethrow original panic
                (Err(__panic), Ok(())) => ::std::panic::resume_unwind(__panic),

                // User succeeded; teardown panicked -> surface teardown panic
                (Ok(_), Err(__panic)) => ::std::panic::resume_unwind(__panic),

                // Both panicked -> prefer original user panic (teardown panic would mask test failure)
                (Err(__panic), Err(_teardown_panic)) => ::std::panic::resume_unwind(__panic),
            }
        }

        __mae_run_test()
    });

    TokenStream::from(quote::quote!(#f),)
}
