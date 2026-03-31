use super::DependencySet;
use crate::scanner::Manifest;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn resolve(root: &Path, manifest: &Manifest) -> Result<DependencySet> {
    let mut deps = DependencySet::default();

    // Look for NuGet packages.lock.json or packages.config
    let lock = root.join("packages.lock.json");
    let config = root.join("packages.config");

    if lock.exists() {
        parse_nuget_lock(&lock, &mut deps);
    } else if config.exists() {
        parse_packages_config(&config, &mut deps);
    }

    // Find .csproj files and read PackageReference elements
    for cfg in &manifest.config_files {
        if cfg.extension().map(|e| e == "csproj").unwrap_or(false) {
            parse_csproj(cfg, root, &mut deps);
        }
    }

    Ok(deps)
}

fn parse_csproj(path: &Path, root: &Path, deps: &mut DependencySet) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Simple XML scan — no full XML parser needed for this heuristic
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<PackageReference") {
            if let Some(include) = extract_xml_attr(trimmed, "Include") {
                let version = extract_xml_attr(trimmed, "Version")
                    .unwrap_or_else(|| "?".to_string());

                // Check if the NuGet global packages folder has this installed
                let pkg_name = format!("{include} v{version}");
                if nuget_package_installed(&include, root) {
                    deps.resolved.push(pkg_name);
                } else {
                    deps.missing.push(pkg_name);
                }
            }
        }
    }
}

fn extract_xml_attr(line: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = line.find(&needle)? + needle.len();
    let end = line[start..].find('"')?;
    Some(line[start..start + end].to_string())
}

fn nuget_package_installed(package: &str, _root: &Path) -> bool {
    // Check the global NuGet cache (Windows default)
    let nuget_root = dirs::home_dir()
        .map(|h| h.join(".nuget").join("packages"))
        .unwrap_or_default();

    if nuget_root.exists() {
        let pkg_dir = nuget_root.join(package.to_lowercase());
        if pkg_dir.exists() {
            return true;
        }
    }
    false
}

fn parse_nuget_lock(path: &Path, deps: &mut DependencySet) {
    // packages.lock.json is just a marker that NuGet restore has been run.
    // If it exists, assume packages are available.
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Extract "resolved" packages from JSON (simple string scan)
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('"') && trimmed.ends_with("\": {") {
            let name = trimmed.trim_matches(|c| c == '"' || c == '{' || c == ':' || c == ' ');
            if !name.is_empty() && name != "dependencies" && name != "version" {
                deps.resolved.push(name.to_string());
            }
        }
    }
}

fn parse_packages_config(path: &Path, deps: &mut DependencySet) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<package ") {
            if let Some(id) = extract_xml_attr(trimmed, "id") {
                let version = extract_xml_attr(trimmed, "version")
                    .unwrap_or_else(|| "?".to_string());
                deps.resolved.push(format!("{id} v{version}"));
            }
        }
    }
}
