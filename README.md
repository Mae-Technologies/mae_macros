# mae_macros

Procedural macros for Mae-Technologies services.

For development rules, see [DEVELOPMENT.md](DEVELOPMENT.md).

---

## Macros

- [`#[run_app]`](#run_app) — Actix-Web server setup wrapper
- [`#[schema]`](#schema) — Postgres schema binding with standard audit columns
- [`#[schema_root]`](#schema_root) — Like `schema` but without `sys_client` FK
- [`#[derive(MaeRepo)]`](#maerepo-derive) — SQL helper type generation
- [`#[mae_test]`](#mae_test) — Async journey test harness

---

## `#[run_app]`

Rewrites a single-expression function body into a complete `async fn run<Context>(…)` Actix-Web server setup.

### What it does

Place `#[run_app]` on a function containing exactly **one** statement — typically a route configuration call. The macro expands it into a full server function that:

- Creates a Redis session store via `app::redis_session`
- Attaches `TracingLogger`, session middleware, and `web::Data` extractors for `PgPool`, `ApplicationBaseUrl`, `HmacSecret`, and the custom context type
- Binds the HTTP server to a `TcpListener` and returns a `Result<Server, anyhow::Error>`

### When to use it

Use `#[run_app]` in your service's `main.rs` or `startup.rs` to replace boilerplate Actix-Web server wiring. The annotated function's body becomes the `.service(…)` / `.configure(…)` call chain appended to the generated app.

### Example

```rust,ignore
use mae_macros::run_app;

#[run_app]
fn configure(cfg: &mut web::ServiceConfig) {
    cfg.configure(routes::register)
}
// Expands to:
// async fn run<Context: Clone + Send + 'static>(
//     listener: TcpListener, db_pool: PgPool, base_url: String,
//     hmac_secret: SecretString, redis_uri: SecretString, custom_context: Context,
// ) -> Result<Server, anyhow::Error> { … }
```

---

## `#[schema]`

Binds a struct to a Postgres schema and injects standard Mae repository columns.

### What it does

`#[schema(CtxType, "schema_name")]` transforms the annotated struct into a full Mae repository by injecting standard audit columns (`id`, `sys_client`, `status`, `comment`, `tags`, `sys_detail`, `created_by`, `updated_by`, `created_at`, `updated_at`) and deriving `MaeRepo`, `sqlx::FromRow`, `serde::Serialize`, `serde::Deserialize`, `Debug`, and `Clone`. It also implements `mae::repo::__private__::Build` with the given schema name.

### When to use it

Use `#[schema]` on every domain repository struct in Mae services. Define only the **domain-specific** fields; all standard columns are injected automatically.

### Example

```rust,ignore
use mae_macros::schema;

#[schema(AppContext, "public")]
pub struct UserRepo {
    pub username: String,
    pub email: String,
}
// Expands to a struct with all standard Mae columns plus `username` and `email`.
// Also generates InsertRow, UpdateRow, Field, PatchField in the same module.
```

---

## `#[schema_root]`

Like [`#[schema]`](#schema) but omits the auto-injected `sys_client` foreign-key column.

### What it does

Identical to `#[schema]` in every respect, except the generated struct does **not** include the `sys_client: i32` field. All other standard audit columns are still injected.

### When to use it

Use `#[schema_root]` for the `sys_client` table itself (or any root entity that has no foreign-key back to `sys_client`).

### Example

```rust,ignore
use mae_macros::schema_root;

#[schema_root(AppContext, "public")]
pub struct SysClientRepo {
    pub name: String,
    pub plan: String,
}
// Like #[schema] but without `sys_client: i32`.
```

---

## `#[derive(MaeRepo)]`

Generates `InsertRow`, `UpdateRow`, `Field`, and `PatchField` types for a repository struct.

### What it does

For each named field in the struct, `MaeRepo` emits:

- **`InsertRow`** — plain struct of all non-`#[locked]`, non-`#[update_only]` fields (used for SQL `INSERT`)
- **`UpdateRow`** — struct of all non-`#[locked]`, non-`#[insert_only]` fields wrapped in `Option<T>` (used for SQL `UPDATE`; `None` fields are omitted)
- **`Field`** — enum with one variant per field plus `All`; `Display` impl emits the column name or a comma-separated list for `SELECT`
- **`PatchField`** — typed enum carrying field values; convertible to `FilterOp<Field>` for partial-update WHERE clauses

### Field attributes

| Attribute | Effect |
|---|---|
| `#[locked]` | Excluded from both `InsertRow` and `UpdateRow` (server-managed: `id`, timestamps) |
| `#[insert_only]` | Excluded from `UpdateRow` only (e.g. `sys_client`) |
| `#[update_only]` | Excluded from `InsertRow` only |

### When to use it

`MaeRepo` is normally applied indirectly via `#[schema]` or `#[schema_root]`. Use it directly only when you need the generated types on a struct that doesn't follow the standard schema layout.

### Example

```rust,ignore
use mae_macros::MaeRepo;

#[derive(MaeRepo, Debug, Clone, sqlx::FromRow)]
pub struct OrderRepo {
    #[locked]
    pub id: i32,
    pub customer_id: i32,
    pub total: i64,
}
// Generates: InsertRow { customer_id, total }
//            UpdateRow { customer_id: Option<i32>, total: Option<i64> }
//            Field { All, id, customer_id, total }
//            PatchField { customer_id(i32), total(i64) }
```

---

## `#[mae_test]`

The standard test harness macro for async journey tests in Mae services.

### What it does

`#[mae_test]` wraps an `async fn` test in a dedicated **multi-threaded Tokio runtime** and
enforces Mae test-hygiene rules at compile time:

- **Forbids** raw `.unwrap()`, `.expect()`, `assert!`, `assert_eq!`, and `assert_ne!` in test bodies.
  Use `must::*` helpers or `?`-propagation instead.
- Drives the async test body synchronously so it integrates cleanly with the standard `#[test]`
  harness (no `#[tokio::test]` needed).
- Supports optional **teardown** (called after the test body, even on panic).
- Supports optional **docker gating** — skip the test at compile time unless
  `MAE_TESTCONTAINERS=1` was set in the environment.

### When to use `#[mae_test]` vs plain `#[test]`

| Scenario | Use |
|---|---|
| Async test that hits a real DB / HTTP endpoint | `#[mae_test]` |
| Test that needs a `TestContext` / `Must` helpers | `#[mae_test]` |
| Pure synchronous unit test with no I/O | `#[test]` |
| Quick in-process logic check | `#[test]` |

Rule of thumb: if your test is `async`, use `#[mae_test]`.

### Attribute arguments

| Argument | Effect |
|---|---|
| _(none)_ | Basic multi-threaded async test |
| `docker` | Skip unless compiled with `MAE_TESTCONTAINERS=1 cargo test` |
| `teardown = <path>` | Call the given `async fn` after the test body, even on panic |

Arguments can be combined: `#[mae_test(docker, teardown = crate::common::context::teardown)]`

---

### Examples

#### Good journey test — uses `TestContext`, `Must`, and `?`

```rust
use mae_macros::mae_test;
use crate::common::{must::Must, context::TestContext};

#[mae_test]
async fn journey_create_user() -> Result<(), anyhow::Error> {
    let ctx = TestContext::new().await?;

    let user = ctx.users().create("alice").await?;
    must_eq(user.name, "alice");

    let fetched = ctx.users().get(user.id).await?;
    must_eq(fetched.id, user.id);

    Ok(())
}
```

#### Good docker-gated test with teardown

```rust
use mae_macros::mae_test;

#[mae_test(docker, teardown = crate::common::context::teardown)]
async fn journey_with_postgres() -> Result<(), anyhow::Error> {
    // Only runs when MAE_TESTCONTAINERS=1 cargo test is used.
    // `teardown` is called even if the test panics.
    let ctx = TestContext::new().await?;
    let result = ctx.some_db_op().await?;
    must_eq(result.status, "ok");
    Ok(())
}
```

#### Bad journey test — will not compile

```rust
use mae_macros::mae_test;

#[mae_test]
async fn bad_test() {
    // compile error: forbidden — use must_be_some() instead
    let x: Option<i32> = None;
    let _ = x.unwrap();

    // compile error: forbidden — return Result and use `?` instead
    assert_eq!(1 + 1, 2);
}
```

---

### Running docker tests

```bash
# Run all tests (non-docker tests only):
cargo +nightly test

# Run all tests including docker-gated tests:
MAE_TESTCONTAINERS=1 cargo +nightly test --features test-utils

# Run a specific test:
MAE_TESTCONTAINERS=1 cargo +nightly test journey_with_postgres
```

> **Note:** `MAE_TESTCONTAINERS=1` is evaluated at **compile time** via `option_env!`. You must
> set it before / during the `cargo test` invocation, not just at runtime.

---

### `#[ignore]` compatibility

`#[mae_test]` preserves all other attributes on the function, including `#[ignore]`:

```rust
#[mae_test]
#[ignore = "WIP: not ready yet"]
async fn pending_journey() -> Result<(), anyhow::Error> {
    Ok(())
}
```

Run ignored tests with `cargo test -- --ignored`.

---

### Re-export path

`#[mae_test]` is re-exported from the `mae` crate's `testing` module (see
`mae::testing::mae_test`). Services should import it via:

```rust
use mae::testing::mae_test;
```

or directly from `mae_macros`:

```rust
use mae_macros::mae_test;
```
