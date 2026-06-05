//! Compile-fail tests for `#[derive(Quiver)]`.
//!
//! To update the expected output, run with the environment variable `TRYBUILD=overwrite`.

#![cfg(feature = "derive")]

#[test]
fn compile_fail() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/compile_fail/*.rs");
}
