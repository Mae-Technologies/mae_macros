# mae_macros

Procedural macros for Mae-Technologies services.

For development rules, see [DEVELOPMENT.md](DEVELOPMENT.md).

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
