use super::DependencySet;
use crate::scanner::Manifest;
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

static INCLUDE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^\s*#\s*include\s+[<"]([^>"]+)[>"]"#).unwrap());

/// Well-known system/SDK include directories on Windows
const WINDOWS_INCLUDE_HINTS: &[&str] = &[
    r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC",
    r"C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC",
    r"C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Tools\MSVC",
    r"C:\Program Files (x86)\Windows Kits\10\Include",
    r"C:\Program Files\LLVM\include",
];

/// vcpkg default install roots
const VCPKG_ROOTS: &[&str] = &[
    r"C:\vcpkg\installed\x64-windows\include",
    r"C:\vcpkg\installed\x86-windows\include",
];

/// Conan default cache (cross-platform)
fn conan_include_hints() -> Vec<PathBuf> {
    let mut hints = Vec::new();
    if let Some(home) = dirs::home_dir() {
        hints.push(home.join(".conan").join("data"));
        hints.push(home.join(".conan2").join("p"));
    }
    hints
}

pub fn resolve(root: &Path, manifest: &Manifest) -> Result<DependencySet> {
    let mut deps = DependencySet::default();

    // Collect all #include directives from project source files
    let mut third_party_includes: HashSet<String> = HashSet::new();
    for src in &manifest.source_files {
        scan_includes(src, &mut third_party_includes);
    }

    // Always add the project root and any "include" subdirectory as an include path
    deps.resolved.push(format!("-I{}", root.display()));
    let project_include = root.join("include");
    if project_include.exists() {
        deps.resolved
            .push(format!("-I{}", project_include.display()));
    }
    // Also add "src" as an include root (common layout)
    let project_src = root.join("src");
    if project_src.exists() {
        deps.resolved
            .push(format!("-I{}", project_src.display()));
    }

    // Check vcpkg
    for vcpkg_path in VCPKG_ROOTS {
        let p = PathBuf::from(vcpkg_path);
        if p.exists() {
            deps.resolved
                .push(format!("-I{}", p.display()));
        }
    }

    // Check conan
    for conan_path in conan_include_hints() {
        if conan_path.exists() {
            deps.resolved
                .push(format!("-I{}", conan_path.display()));
        }
    }

    // Walk Windows VS include dirs (only relevant on Windows, but safe to check)
    for hint in WINDOWS_INCLUDE_HINTS {
        let p = PathBuf::from(hint);
        if p.exists() {
            // The MSVC path has a version subdirectory — grab the latest
            if let Ok(entries) = std::fs::read_dir(&p) {
                let mut versions: Vec<PathBuf> = entries
                    .flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.path())
                    .collect();
                versions.sort();
                if let Some(latest) = versions.last() {
                    let inc = latest.join("include");
                    if inc.exists() {
                        deps.resolved.push(format!("-I{}", inc.display()));
                    }
                }
            } else {
                deps.resolved.push(format!("-I{}", p.display()));
            }
        }
    }

    // Check for third-party headers that we couldn't resolve
    let stdlib_headers: HashSet<&str> = STDLIB_HEADERS.iter().copied().collect();
    for header in &third_party_includes {
        // Strip path prefix for stdlib check
        let base = header.split('/').next_back().unwrap_or(header);
        if !stdlib_headers.contains(base) && !stdlib_headers.contains(header.as_str()) {
            // Try to find it under known include dirs
            if !header_found_on_path(header, root) {
                deps.missing.push(format!("#{header}"));
            }
        }
    }

    Ok(deps)
}

fn scan_includes(path: &Path, set: &mut HashSet<String>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };
    for line in content.lines() {
        if let Some(cap) = INCLUDE_RE.captures(line) {
            set.insert(cap[1].to_string());
        }
    }
}

fn header_found_on_path(header: &str, root: &Path) -> bool {
    // Check project-local include and src dirs
    for subdir in &["include", "src", "third_party", "vendor", "extern", "lib"] {
        if root.join(subdir).join(header).exists() {
            return true;
        }
    }
    // Check root itself
    if root.join(header).exists() {
        return true;
    }
    false
}

/// Standard library headers — safe to ignore as "missing"
const STDLIB_HEADERS: &[&str] = &[
    "stdio.h", "stdlib.h", "string.h", "math.h", "time.h", "errno.h",
    "stdint.h", "stdbool.h", "assert.h", "ctype.h", "limits.h", "float.h",
    "stddef.h", "stdarg.h", "signal.h", "setjmp.h", "locale.h",
    // POSIX
    "unistd.h", "fcntl.h", "sys/types.h", "sys/stat.h", "pthread.h",
    // Windows
    "windows.h", "winsock2.h", "ws2tcpip.h",
    // C++ stdlib (no extension)
    "iostream", "vector", "string", "map", "set", "unordered_map", "unordered_set",
    "algorithm", "memory", "functional", "thread", "mutex", "chrono",
    "fstream", "sstream", "stdexcept", "cassert", "cmath", "cstdlib",
    "cstring", "cstdio", "cstdint", "climits", "cfloat", "optional",
    "variant", "any", "tuple", "array", "deque", "list", "queue",
    "stack", "bitset", "iterator", "numeric", "random", "regex",
    "filesystem", "atomic", "future", "condition_variable", "span",
    "ranges", "concepts", "type_traits", "utility", "format", "print",
];
