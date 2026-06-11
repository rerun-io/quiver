//! Compile-fail tests for `#[derive(Quiver)]`.
//!
//! To update the expected output, run with the environment variable `TRYBUILD=overwrite`.

// Skipped under Miri: trybuild spawns the compiler per case (which Miri can't
// run as a subprocess), it's slow, and it only checks diagnostics — there is no
// `unsafe` here for Miri to validate.
#![cfg(all(feature = "derive", not(miri)))]

#[test]
fn compile_fail() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/compile_fail/*.rs");
}
