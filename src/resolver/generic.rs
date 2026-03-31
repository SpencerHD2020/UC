use super::DependencySet;
use crate::scanner::Manifest;
use anyhow::Result;
use std::path::Path;

/// For languages where uc delegates to native tooling (Rust → cargo, Go → go build, etc.),
/// dependency resolution is handled by that toolchain. We still surface lock file status.
pub fn resolve(root: &Path, manifest: &Manifest) -> Result<DependencySet> {
    let mut deps = DependencySet::default();

    // Cargo.lock
    if root.join("Cargo.lock").exists() {
        deps.resolved
            .push("Cargo.lock found — dependencies managed by Cargo".into());
    }

    // go.sum
    if root.join("go.sum").exists() {
        deps.resolved
            .push("go.sum found — dependencies managed by Go modules".into());
    }

    // package-lock.json / yarn.lock / pnpm-lock.yaml
    if root.join("package-lock.json").exists() {
        deps.resolved
            .push("package-lock.json found — dependencies managed by npm".into());
    } else if root.join("yarn.lock").exists() {
        deps.resolved
            .push("yarn.lock found — dependencies managed by Yarn".into());
    } else if root.join("pnpm-lock.yaml").exists() {
        deps.resolved
            .push("pnpm-lock.yaml found — dependencies managed by pnpm".into());
    } else {
        // Check for package.json without a lock — might need install
        if manifest
            .config_files
            .iter()
            .any(|f| f.file_name().map(|n| n == "package.json").unwrap_or(false))
        {
            deps.missing
                .push("No lock file found — run `npm install` first".into());
        }
    }

    // pyproject.toml / requirements.txt
    if root.join("requirements.txt").exists() {
        deps.resolved
            .push("requirements.txt found".into());
    }
    if root.join("Pipfile.lock").exists() {
        deps.resolved
            .push("Pipfile.lock found — dependencies managed by Pipenv".into());
    }

    Ok(deps)
}
