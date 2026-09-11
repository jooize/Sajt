//! Stamp the commit a build came from into the binary.
//!
//! `sajt --version` prints the crate version, and, when the builder said
//! which commit it was building, that commit after it. Whoever knows the
//! commit passes it in `SAJT_COMMIT`: the flake package hands over the
//! flake's own revision, and the release workflow then asserts that the
//! binary it just built names the commit being released. Without that
//! assertion a release could carry any bytes at all.
//!
//! This script deliberately never runs `git`. A build script that reads the
//! working tree makes the build depend on state outside its declared inputs,
//! which is what a reproducible build must not do, and the Nix sandbox has
//! no repository to read anyway. With no `SAJT_COMMIT` the version string is
//! the bare crate version, which is what an ordinary `cargo build` on a
//! workstation wants.

fn main() {
    // The only input that can change this script's output. Naming it also
    // stops Cargo re-running the script on every unrelated source edit.
    println!("cargo:rerun-if-env-changed=SAJT_COMMIT");

    // Cargo sets this for every build script; there is no build without it.
    let version = std::env::var("CARGO_PKG_VERSION").expect("Cargo sets CARGO_PKG_VERSION");

    // An empty or blank value counts as "not given": a CI step defaulting a
    // variable to the empty string must not produce a version reading "()".
    let stamped = match std::env::var("SAJT_COMMIT") {
        Ok(commit) if !commit.trim().is_empty() => format!("{} ({})", version, commit.trim()),
        _ => version,
    };

    println!("cargo:rustc-env=SAJT_VERSION={}", stamped);
}
