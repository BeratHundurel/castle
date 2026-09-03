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
    optional: bool,
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
                "board",
                "command_palette",
                "document_editor",
                "quick_capture",
                "runtime",
                "settings",
                "shell",
                "storage",
            ]),
        ),
        ("runtime", set(&["storage"])),
        ("settings", set(&[])),
        ("board", set(&["runtime", "storage", "workspace"])),
        ("command_palette", set(&["runtime", "settings", "storage"])),
        ("quick_capture", set(&["runtime", "storage"])),
        ("castle-mcp", set(&["storage"])),
        (
            "document_editor",
            set(&["runtime", "settings", "storage", "workspace"]),
        ),
        ("entity", set(&[])),
        ("migration", set(&[])),
        (
            "shell",
            set(&[
                "board",
                "command_palette",
                "document_editor",
                "runtime",
                "settings",
                "storage",
                "workspace",
            ]),
        ),
        ("storage", set(&["entity", "migration"])),
        ("test_support", set(&[])),
        ("workspace", set(&["runtime", "settings", "storage"])),
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
        "runtime",
        "settings",
        "board",
        "castle-mcp",
        "command_palette",
        "quick_capture",
        "document_editor",
        "shell",
        "workspace",
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

fn runtime_dependencies(package: &CargoPackage) -> impl Iterator<Item = &CargoDependency> {
    production_dependencies(package).filter(|dependency| !dependency.optional)
}

fn assert_excludes_production_dependencies(package: &CargoPackage, forbidden: &[&str]) {
    let dependencies = runtime_dependencies(package)
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
