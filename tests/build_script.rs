//! Runs the tests of `build.rs`, which cargo never compiles as a test target.
#[path = "../build.rs"]
#[expect(dead_code, reason = "only the tests of the build script run here, not its steps")]
mod build_script;
