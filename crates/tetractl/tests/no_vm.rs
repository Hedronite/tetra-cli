use std::fs;
use std::path::PathBuf;

#[test]
fn lockfile_has_expected_vm_boundary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lock =
        fs::read_to_string(manifest_dir.join("../../Cargo.lock")).expect("workspace Cargo.lock");
    let nickel = lock.matches("name = \"nickel-lang-core\"").count();
    let hedron_ncl = lock.matches("name = \"hedron-ncl\"").count();
    let rusqlite = lock.matches("name = \"rusqlite\"").count();
    assert_eq!(
        nickel, 1,
        "workspace Cargo.lock must contain exactly one nickel-lang-core package, found {nickel}"
    );
    assert_eq!(
        hedron_ncl, 1,
        "workspace Cargo.lock must contain exactly one hedron-ncl package, found {hedron_ncl}"
    );
    assert_eq!(
        rusqlite, 0,
        "workspace Cargo.lock must contain zero rusqlite packages, found {rusqlite}"
    );

    let manifest =
        fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("tetractl Cargo.toml");
    assert!(
        manifest.contains("hedron-ncl"),
        "crates/tetractl/Cargo.toml must list hedron-ncl"
    );
    assert!(
        !manifest.contains("nickel-lang-core"),
        "crates/tetractl/Cargo.toml must not list nickel-lang-core"
    );
}
