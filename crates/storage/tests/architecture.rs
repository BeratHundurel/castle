use std::{fs, path::Path};

#[test]
fn app_and_mcp_production_sources_do_not_depend_on_database_implementation_crates() {
    let Some(workspace) = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
    else {
        panic!("storage crate should be inside the workspace");
    };

    for crate_name in ["app", "mcp_server"] {
        let crate_root = workspace.join("crates").join(crate_name);
        assert_manifest_has_no_production_database_dependency(&crate_root.join("Cargo.toml"));
        assert_sources_have_no_production_database_imports(&crate_root.join("src"));
    }
}

fn assert_manifest_has_no_production_database_dependency(manifest_path: &Path) {
    let manifest = fs::read_to_string(manifest_path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", manifest_path.display()));
    let production_dependencies = manifest
        .split("[dev-dependencies]")
        .next()
        .unwrap_or(&manifest);
    for dependency in ["entity", "sea-orm", "migration"] {
        assert!(
            !production_dependencies.lines().any(|line| line
                .trim_start()
                .starts_with(&format!("{dependency}."))
                || line.trim_start().starts_with(&format!("{dependency} ="))),
            "{} has a production dependency on {dependency}",
            manifest_path.display()
        );
    }
}

fn assert_sources_have_no_production_database_imports(directory: &Path) {
    for entry in fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", directory.display()))
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => panic!("source directory entry should be readable: {error}"),
        };
        let path = entry.path();
        if path.is_dir() {
            assert_sources_have_no_production_database_imports(&path);
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }
        if path.file_name().and_then(|name| name.to_str()) == Some("tests.rs") {
            continue;
        }

        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        for forbidden in ["entity::", "sea_orm::", "migration::"] {
            assert!(
                !production.contains(forbidden),
                "{} imports {forbidden} in production code",
                path.display()
            );
        }
        for forbidden in [".store().connection()", "store.connection()"] {
            assert!(
                !production.contains(forbidden),
                "{} bypasses the Store boundary with {forbidden}",
                path.display()
            );
        }
    }
}
