use super::{Detection, DetectionTier, Language};
use crate::scanner::Manifest;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Config-file detection (most reliable tier)
// ---------------------------------------------------------------------------

pub fn detect_from_config(manifest: &Manifest) -> Option<Detection> {
    for path in &manifest.config_files {
        let name = path.file_name()?.to_str()?;
        let name_lower = name.to_lowercase();

        let (lang, note) = match name_lower.as_str() {
            "cargo.toml" => (Language::Rust, "Cargo.toml found"),
            "go.mod" => (Language::Go, "go.mod found"),
            "pom.xml" => (Language::Java, "pom.xml (Maven) found"),
            "build.gradle" | "build.gradle.kts" => (Language::Java, "Gradle build file found"),
            "build.xml" => (Language::Java, "build.xml (Ant) found"),
            "pyproject.toml" | "setup.py" | "setup.cfg" => (Language::Python, "Python project file found"),
            "package.json" => {
                // Could be JS or TS — check for tsconfig.json
                let has_ts = manifest
                    .config_files
                    .iter()
                    .any(|f| f.file_name().map(|n| n == "tsconfig.json").unwrap_or(false));
                if has_ts {
                    (Language::TypeScript, "package.json + tsconfig.json found")
                } else {
                    (Language::JavaScript, "package.json found")
                }
            }
            "tsconfig.json" => (Language::TypeScript, "tsconfig.json found"),
            "cmakelists.txt" => {
                // CMake could be C or C++ — inspect source files to decide
                let lang = detect_cmake_language(manifest);
                (lang, "CMakeLists.txt found")
            }
            _ => {
                // Check extensions: *.csproj, *.sln → C#
                if name_lower.ends_with(".csproj") || name_lower.ends_with(".sln") {
                    (Language::CSharp, ".csproj/.sln found")
                } else if name_lower.ends_with(".vcxproj") {
                    (Language::Cpp, ".vcxproj (Visual C++) found")
                } else {
                    continue;
                }
            }
        };

        return Some(Detection {
            language: lang,
            tier: DetectionTier::Heuristic,
            confidence_notes: vec![note.to_string()],
        });
    }
    None
}

fn detect_cmake_language(manifest: &Manifest) -> Language {
    let cpp_exts = ["cc", "cpp", "cxx", "c++", "hh", "hpp", "hxx"];
    for path in &manifest.source_files {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if cpp_exts.contains(&ext.to_lowercase().as_str()) {
                return Language::Cpp;
            }
        }
    }
    Language::C
}

// ---------------------------------------------------------------------------
// Import / include pattern detection
// ---------------------------------------------------------------------------

static INCLUDE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^\s*#\s*include\s+[<"]([^>"]+)[>"]"#).unwrap());

static JAVA_IMPORT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*import\s+(static\s+)?([a-zA-Z_][\w.]+)").unwrap());

static CS_USING_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*using\s+([a-zA-Z_][\w.]+)\s*;").unwrap());

/// Score map: library prefix → (language, weight)
static CPP_STDLIB: Lazy<Vec<&str>> = Lazy::new(|| {
    vec![
        "iostream", "vector", "string", "map", "set", "algorithm", "memory",
        "functional", "thread", "mutex", "chrono", "fstream", "sstream",
        "stdexcept", "cassert", "cmath", "cstdlib", "cstring", "cstdio",
    ]
});

static C_STDLIB: Lazy<Vec<&str>> = Lazy::new(|| {
    vec![
        "stdio.h", "stdlib.h", "string.h", "math.h", "time.h", "errno.h",
        "stdint.h", "stdbool.h", "assert.h", "ctype.h", "limits.h",
    ]
});

pub fn detect_from_imports(manifest: &Manifest) -> Option<Detection> {
    let mut lang_scores: HashMap<String, i32> = HashMap::new();

    // Sample up to the first 20 source files
    for path in manifest.source_files.iter().take(20) {
        score_file(path, &mut lang_scores);
    }

    if lang_scores.is_empty() {
        return None;
    }

    let mut ranked: Vec<(String, i32)> = lang_scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));

    let (winner, score) = &ranked[0];
    if *score == 0 {
        return None;
    }

    let lang = match winner.as_str() {
        "cpp" => Language::Cpp,
        "c" => Language::C,
        "java" => Language::Java,
        "csharp" => Language::CSharp,
        _ => return None,
    };

    Some(Detection {
        language: lang,
        tier: DetectionTier::Heuristic,
        confidence_notes: vec![format!("Import analysis score: {score}")],
    })
}

fn score_file(path: &Path, scores: &mut HashMap<String, i32>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    for line in content.lines().take(50) {
        // C/C++ includes
        if let Some(cap) = INCLUDE_RE.captures(line) {
            let header = &cap[1];
            if C_STDLIB.contains(&header) {
                *scores.entry("c".into()).or_insert(0) += 2;
            } else if CPP_STDLIB.contains(&header) {
                *scores.entry("cpp".into()).or_insert(0) += 3;
            } else {
                // Unknown include — lean C if only .h, C++ if no extension
                if header.ends_with(".h") {
                    *scores.entry("c".into()).or_insert(0) += 1;
                } else {
                    *scores.entry("cpp".into()).or_insert(0) += 1;
                }
            }
        }

        // Java imports
        if JAVA_IMPORT_RE.is_match(line) {
            *scores.entry("java".into()).or_insert(0) += 2;
        }

        // C# usings
        if CS_USING_RE.is_match(line) {
            *scores.entry("csharp".into()).or_insert(0) += 2;
        }
    }
}
