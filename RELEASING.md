# Releasing Castle

Castle releases are built by GitHub Actions for Windows x86-64 and ARM64. Each release contains standalone Castle and Castle MCP executables, an MSI installer, and SHA-256 checksums.

## Publish a release

1. Update `version` under `[workspace.package]` in `Cargo.toml`.
2. Refresh the lockfile and verify the release build:

   ```sh
   cargo check
   cargo build --release --bin castle --bin castle-mcp
   ```

3. Commit the version change:

   ```sh
   git add Cargo.toml Cargo.lock
   git commit -m "release: vX.Y.Z"
   ```

4. Tag that commit and push the commit and tag:

   ```sh
   git tag vX.Y.Z
   git push origin HEAD
   git push origin vX.Y.Z
   ```

The `Release` workflow checks that the tag matches the Cargo version, builds both architectures on native GitHub runners, packages the MSI installers, and creates the GitHub release with generated release notes.

To rerun publishing for an existing tag, open **Actions → Release → Run workflow** and enter the tag. Existing assets with the same names are replaced.

## Version requirements

Release tags must use `vMAJOR.MINOR.PATCH`, for example `v0.2.0`. The tag without its leading `v` must exactly match the workspace version in `Cargo.toml`.

## Signing

Artifacts are currently unsigned. Before distributing Castle broadly, add Authenticode signing for both the executable and MSI in the release workflow to avoid Windows SmartScreen warnings.
