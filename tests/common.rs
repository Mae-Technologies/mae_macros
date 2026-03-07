#[cfg(test)]
use pretty_assertions::{assert_eq, assert_ne};
use std::panic::Location;

/// Trait for safe test assertions on `Option` and `Result`.
#[cfg(test)]
pub trait Must<T> {
    #[track_caller]
    fn must(self) -> T;
}

#[cfg(test)]
pub trait MustExpect<T>: Sized {
    #[track_caller]
    fn must_expect(self, msg: &str) -> T;
}

#[cfg(test)]
impl<T> Must<T> for Option<T> {
    #[track_caller]
    fn must(self) -> T {
        self.unwrap_or_else(|| {
            panic!("test invariant failed: expected Some, got None at {}", Location::caller())
        })
    }
}

#[cfg(test)]
impl<T> MustExpect<T> for Option<T> {
    #[track_caller]
    fn must_expect(self, msg: &str) -> T {
        self.unwrap_or_else(|| {
            panic!("{} (expected Some, got None) at {}", msg, Location::caller())
        })
    }
}

#[cfg(test)]
impl<T, E: std::fmt::Debug> Must<T> for Result<T, E> {
    #[track_caller]
    fn must(self) -> T {
        self.unwrap_or_else(|err| {
            panic!("test invariant failed: expected Ok, got {:?} at {}", err, Location::caller())
        })
    }
}

#[cfg(test)]
impl<T, E: std::fmt::Debug> MustExpect<T> for Result<T, E> {
    #[track_caller]
    fn must_expect(self, msg: &str) -> T {
        self.unwrap_or_else(|err| {
            panic!("{} (expected Ok, got {:?}) at {}", msg, err, Location::caller())
        })
    }
}

#[cfg(test)]
#[track_caller]
pub fn must_be_some<T>(opt: Option<T>) -> T { opt.must() }

#[cfg(test)]
#[track_caller]
pub fn must_be_ok<T, E: std::fmt::Debug>(res: Result<T, E>) -> T { res.must() }

#[cfg(test)]
#[track_caller]
pub fn must_expect_some<T>(opt: Option<T>, msg: &str) -> T { opt.must_expect(msg) }

#[cfg(test)]
#[track_caller]
pub fn must_expect_ok<T, E: std::fmt::Debug>(res: Result<T, E>, msg: &str) -> T { res.must_expect(msg) }

#[allow(clippy::disallowed_methods)]
#[track_caller]
pub fn must_eq<V: PartialEq + std::fmt::Debug>(left: V, right: V) { assert_eq!(left, right); }

#[track_caller]
pub fn must_ne<V: PartialEq + std::fmt::Debug>(left: V, right: V) { assert_ne!(left, right); }

#[track_caller]
pub fn must_be_true(b: bool) { assert!(b); }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_must_some_ok() { must_eq(Some(42).must(), 42); }

    #[test]
    #[should_panic(expected = "expected Some, got None")]
    fn option_must_none_panics() { let _: i32 = None.must(); }

    #[test]
    fn result_must_ok() { must_eq(Ok::<i32, &str>(7).must(), 7); }

    #[test]
    #[should_panic(expected = "expected Ok")]
    fn result_must_err_panics() { let _: i32 = Err::<i32, &str>("boom").must(); }

    #[test]
    fn free_helpers_work() {
        let a = must_be_some(Some(1));
        let b = must_be_ok::<_, &str>(Ok(2));
        let c = must_expect_some(Some(3), "msg");
        let d = must_expect_ok::<_, &str>(Ok(4), "msg");
        must_eq(a + b + c + d, 10);
    }

    #[test]
    fn must_eq_and_ne() { must_eq(5, 5); must_ne(5, 6); }

    #[test]
    fn must_be_true_works() { must_be_true(true); }

    #[test]
    #[should_panic]
    fn must_be_true_panics_on_false() { must_be_true(false); }
}
