use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    workspace_root: PathBuf,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
    manifest_path: PathBuf,
    dependencies: Vec<CargoDependency>,
}

#[derive(Debug, Deserialize)]
struct CargoDependency {
    name: String,
    kind: Option<String>,
}

static CARGO_METADATA: OnceLock<CargoMetadata> = OnceLock::new();

#[test]
fn workspace_packages_follow_the_allowed_dependency_graph() {
    let metadata = cargo_metadata();
    let workspace_packages = workspace_packages(metadata);
    let allowed_local_dependencies = BTreeMap::from([
        (
            "app",
            set(&[
                "app_services",
                "app_settings",
                "board",
                "document_editor",
                "storage",
                "workspace_ui",
            ]),
        ),
        ("app_services", set(&["storage"])),
        ("app_settings", set(&[])),
        ("board", set(&["app_services", "storage", "workspace_ui"])),
        ("castle-mcp", set(&["storage", "workspace_api"])),
        (
            "document_editor",
            set(&["app_services", "app_settings", "storage", "workspace_ui"]),
        ),
        ("entity", set(&[])),
        ("migration", set(&[])),
        ("storage", set(&["entity", "migration", "workspace_api"])),
        ("test_support", set(&[])),
        ("workspace_api", set(&[])),
        ("workspace_ui", set(&["storage"])),
    ]);

    assert_eq!(
        workspace_packages.keys().copied().collect::<BTreeSet<_>>(),
        allowed_local_dependencies
            .keys()
            .copied()
            .collect::<BTreeSet<_>>(),
        "the architecture allow-list must cover every workspace package"
    );

    for (package_name, package) in workspace_packages {
        let actual = production_dependencies(package)
            .filter(|dependency| allowed_local_dependencies.contains_key(dependency.name.as_str()))
            .map(|dependency| dependency.name.as_str())
            .collect::<BTreeSet<_>>();
        let expected = allowed_local_dependencies
            .get(package_name)
            .expect("workspace package should have an architecture entry");

        assert_eq!(
            &actual, expected,
            "{package_name} has unexpected local production dependency edges"
        );
    }
}

#[test]
fn persistence_and_protocol_dependencies_stay_in_their_own_layers() {
    let metadata = cargo_metadata();
    let workspace_packages = workspace_packages(metadata);

    for package_name in [
        "app",
        "app_services",
        "app_settings",
        "board",
        "castle-mcp",
        "document_editor",
        "workspace_api",
        "workspace_ui",
    ] {
        assert_excludes_production_dependencies(
            workspace_packages
                .get(package_name)
                .expect("consumer package should exist"),
            &["entity", "migration", "sea-orm"],
        );
    }

    assert_excludes_production_dependencies(
        workspace_packages
            .get("storage")
            .expect("storage package should exist"),
        &["rmcp", "schemars"],
    );
    assert_excludes_production_dependencies(
        workspace_packages
            .get("workspace_api")
            .expect("workspace API package should exist"),
        &[
            "app",
            "app_services",
            "board",
            "document_editor",
            "entity",
            "gpui",
            "migration",
            "rmcp",
            "sea-orm",
            "storage",
            "workspace_ui",
        ],
    );
}

fn cargo_metadata() -> &'static CargoMetadata {
    CARGO_METADATA.get_or_init(|| {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("storage crate should be inside the workspace");
        let output = Command::new(env!("CARGO"))
            .args([
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--manifest-path",
            ])
            .arg(workspace.join("Cargo.toml"))
            .output()
            .expect("cargo metadata should run");

        assert!(
            output.status.success(),
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("cargo metadata should return valid JSON")
    })
}

fn workspace_packages(metadata: &CargoMetadata) -> BTreeMap<&str, &CargoPackage> {
    metadata
        .packages
        .iter()
        .filter(|package| package.manifest_path.starts_with(&metadata.workspace_root))
        .map(|package| (package.name.as_str(), package))
        .collect()
}

fn production_dependencies(package: &CargoPackage) -> impl Iterator<Item = &CargoDependency> {
    package
        .dependencies
        .iter()
        .filter(|dependency| dependency.kind.as_deref() != Some("dev"))
}

fn assert_excludes_production_dependencies(package: &CargoPackage, forbidden: &[&str]) {
    let dependencies = production_dependencies(package)
        .map(|dependency| dependency.name.as_str())
        .collect::<BTreeSet<_>>();
    for dependency in forbidden {
        assert!(
            !dependencies.contains(dependency),
            "{} has forbidden production dependency {dependency}",
            package.name
        );
    }
}

fn set<'a>(items: &'a [&'a str]) -> BTreeSet<&'a str> {
    items.iter().copied().collect()
}
