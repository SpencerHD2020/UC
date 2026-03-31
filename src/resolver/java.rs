use super::DependencySet;
use crate::scanner::Manifest;
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

static IMPORT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*import\s+(?:static\s+)?([a-zA-Z_][\w.]+)").unwrap());

/// Maven local repo default
fn maven_local_repo() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".m2")
        .join("repository")
}

pub fn resolve(root: &Path, manifest: &Manifest) -> Result<DependencySet> {
    let mut deps = DependencySet::default();

    // Collect all import statements
    let mut packages: HashSet<String> = HashSet::new();
    for src in &manifest.source_files {
        scan_imports(src, &mut packages);
    }

    // Try to find JARs in the Maven local repo
    let m2 = maven_local_repo();
    let mut classpath_entries: Vec<String> = Vec::new();

    // Local lib/ directory (common simple project layout)
    let lib_dir = root.join("lib");
    if lib_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&lib_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().map(|e| e == "jar").unwrap_or(false) {
                    let s = p.display().to_string();
                    deps.resolved.push(s.clone());
                    classpath_entries.push(s);
                }
            }
        }
    }

    // Maven repo lookup for well-known packages
    for pkg in &packages {
        if is_jdk_package(pkg) {
            continue;
        }
        if let Some(jar) = find_jar_in_m2(pkg, &m2) {
            let s = jar.display().to_string();
            if !classpath_entries.contains(&s) {
                deps.resolved.push(s.clone());
                classpath_entries.push(s);
            }
        } else if !is_jdk_package(pkg) {
            // Only report as missing if it's not a project-internal package
            let is_project_pkg = manifest.source_files.iter().any(|f| {
                let rel = f
                    .strip_prefix(root)
                    .map(|r| r.to_string_lossy().replace('\\', "."))
                    .unwrap_or_default();
                rel.contains(pkg.replace('.', "/").as_str())
            });
            if !is_project_pkg {
                deps.missing.push(pkg.clone());
            }
        }
    }

    deps.classpath = classpath_entries;
    Ok(deps)
}

fn scan_imports(path: &Path, set: &mut HashSet<String>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };
    for line in content.lines() {
        if let Some(cap) = IMPORT_RE.captures(line) {
            // Take the top-level package (first two segments) for deduplication
            let full = cap[1].to_string();
            let top_level: String = full.splitn(3, '.').take(2).collect::<Vec<_>>().join(".");
            set.insert(top_level);
        }
    }
}

fn is_jdk_package(pkg: &str) -> bool {
    matches!(
        pkg.split('.').next().unwrap_or(""),
        "java" | "javax" | "sun" | "com.sun" | "jdk" | "org.xml" | "org.w3c"
    ) || pkg.starts_with("java.")
      || pkg.starts_with("javax.")
}

/// Very naive: look for any JAR under the group/artifact path in .m2
fn find_jar_in_m2(pkg: &str, m2: &Path) -> Option<PathBuf> {
    if !m2.exists() {
        return None;
    }
    // pkg is like "org.apache" — walk two levels deep
    let parts: Vec<&str> = pkg.split('.').collect();
    let mut search_root = m2.to_path_buf();
    for part in &parts {
        search_root = search_root.join(part);
        if !search_root.exists() {
            return None;
        }
    }
    // Find first JAR anywhere under this dir
    find_first_jar(&search_root)
}

fn find_first_jar(dir: &Path) -> Option<PathBuf> {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().map(|e| e == "jar").unwrap_or(false) {
                return Some(p);
            }
            if p.is_dir() {
                if let Some(j) = find_first_jar(&p) {
                    return Some(j);
                }
            }
        }
    }
    None
}
