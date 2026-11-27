use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn lockfile_has_expected_vm_boundary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lock =
        fs::read_to_string(manifest_dir.join("../../Cargo.lock")).expect("workspace Cargo.lock");
    let nickel = lock.matches("name = \"nickel-lang-core\"").count();
    let hedron_ncl = lock.matches("name = \"hedron-ncl\"").count();
    let rusqlite = lock.matches("name = \"rusqlite\"").count();
    let lattice = lock.matches("name = \"facet-lattice\"").count();
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
    assert_eq!(
        lattice, 0,
        "workspace Cargo.lock must contain zero facet-lattice packages, found {lattice}"
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

#[test]
fn in_process_modules_do_not_spawn() {
    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    for name in ["ncl.rs", "eval.rs", "pack.rs", "gen.rs"] {
        let src = fs::read_to_string(src_dir.join(name)).unwrap();
        let process = ["std", "process"].join("::");
        let spawn = ["Command", "new"].join("::");
        assert!(!src.contains(&process), "{name} must not mention {process}");
        assert!(!src.contains(&spawn), "{name} must not spawn {spawn}");
    }
}

#[test]
fn no_nickel_cli_subprocess_in_src_trees() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let roots = [manifest.join("src"), manifest.join("../hedron-ncl/src")];
    let mut hits = Vec::new();
    for root in roots {
        walk_rs(&root, &mut |path, text| {
            if looks_like_nickel_cli(text) {
                hits.push(path.display().to_string());
            }
        });
    }
    assert!(hits.is_empty(), "nickel CLI subprocess spawn in: {hits:?}");
}

fn walk_rs(dir: &Path, visit: &mut dyn FnMut(&Path, &str)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            walk_rs(&path, visit);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let text = fs::read_to_string(&path).unwrap();
            visit(&path, &text);
        }
    }
}

fn looks_like_nickel_cli(text: &str) -> bool {
    let spawn = ["Command", "new"].join("::");
    if !text.contains(&spawn) {
        return false;
    }
    text.contains("Command::new(\"nickel\")")
        || text.contains("Command::new(\"ncl\")")
        || text.contains(".arg(\"nickel\")")
        || text.contains("\"nickel check\"")
        || text.contains("\"nickel export\"")
}
