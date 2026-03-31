use anyhow::Result;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Everything uc knows about the files in a project.
#[derive(Debug)]
pub struct Manifest {
    pub root: PathBuf,
    /// Source files that might need compilation
    pub source_files: Vec<PathBuf>,
    /// Config/manifest files (pom.xml, *.csproj, CMakeLists.txt, …)
    pub config_files: Vec<PathBuf>,
    /// Every file in the project
    pub all_files: Vec<PathBuf>,
    /// Number of unique directories containing source files
    pub dir_count: usize,
}

/// File extensions we treat as compilable source
const SOURCE_EXTENSIONS: &[&str] = &[
    // C / C++
    "c", "cc", "cpp", "cxx", "c++",
    "h", "hh", "hpp", "hxx",
    // C#
    "cs",
    // Java
    "java",
    // Rust
    "rs",
    // Go
    "go",
    // Python
    "py",
    // JavaScript / TypeScript
    "js", "mjs", "cjs", "ts", "mts",
    // Zig
    "zig",
    // Swift
    "swift",
    // Kotlin
    "kt", "kts",
];

/// Files that give us build-system or language hints
const CONFIG_NAMES: &[&str] = &[
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "CMakeLists.txt",
    "Makefile",
    "makefile",
    "GNUmakefile",
    "Cargo.toml",
    "go.mod",
    "package.json",
    "tsconfig.json",
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "*.csproj",
    "*.sln",
    "*.vcxproj",
    "build.xml",   // Ant
];

/// Directories we always skip
const SKIP_DIRS: &[&str] = &[
    ".git", ".hg", ".svn",
    "node_modules",
    "target",       // Rust / Maven
    "build",
    "dist",
    "out",
    ".idea", ".vscode",
    "__pycache__",
    ".mypy_cache",
    "venv", ".venv", "env",
    "bin", "obj",   // .NET
    ".gradle",
];

pub fn scan(root: &Path) -> Result<Manifest> {
    let mut source_files: Vec<PathBuf> = Vec::new();
    let mut config_files: Vec<PathBuf> = Vec::new();
    let mut all_files: Vec<PathBuf> = Vec::new();
    let mut source_dirs: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    let gitignore_patterns = load_gitignore_patterns(root);

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !should_skip_dir(e))
    {
        let entry = entry?;
        let path = entry.path().to_path_buf();

        if entry.file_type().is_dir() {
            continue;
        }

        // Check against .gitignore
        if is_gitignored(&path, root, &gitignore_patterns) {
            continue;
        }

        all_files.push(path.clone());

        // Check if this is a known config file
        if is_config_file(&path) {
            config_files.push(path.clone());
        }

        // Check if this is a compilable source file
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if SOURCE_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                if let Some(parent) = path.parent() {
                    source_dirs.insert(parent.to_path_buf());
                }
                source_files.push(path);
            }
        }
    }

    // Sort for deterministic output
    source_files.sort();
    config_files.sort();
    all_files.sort();

    let dir_count = source_dirs.len();

    Ok(Manifest {
        root: root.to_path_buf(),
        source_files,
        config_files,
        all_files,
        dir_count,
    })
}

fn should_skip_dir(entry: &walkdir::DirEntry) -> bool {
    if entry.file_type().is_dir() {
        if let Some(name) = entry.file_name().to_str() {
            return SKIP_DIRS.contains(&name);
        }
    }
    false
}

fn is_config_file(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return false,
    };

    for pattern in CONFIG_NAMES {
        if pattern.starts_with('*') {
            // Simple glob: check extension
            let ext = &pattern[1..]; // e.g. ".csproj"
            if name.ends_with(ext) {
                return true;
            }
        } else if name.eq_ignore_ascii_case(pattern) {
            return true;
        }
    }
    false
}

/// Minimal .gitignore reader — handles simple patterns only.
fn load_gitignore_patterns(root: &Path) -> Vec<String> {
    let gitignore = root.join(".gitignore");
    if !gitignore.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(gitignore)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect()
}

fn is_gitignored(path: &Path, root: &Path, patterns: &[String]) -> bool {
    let rel = match path.strip_prefix(root) {
        Ok(r) => r.to_string_lossy().replace('\\', "/"),
        Err(_) => return false,
    };

    for pattern in patterns {
        // Very lightweight glob check — handles the most common patterns.
        let p = pattern.trim_start_matches('/');
        if rel.starts_with(p) || rel.contains(&format!("/{p}")) {
            return true;
        }
        // Extension wildcard like *.log
        if let Some(ext) = pattern.strip_prefix("*.") {
            if rel.ends_with(&format!(".{ext}")) {
                return true;
            }
        }
    }
    false
}
