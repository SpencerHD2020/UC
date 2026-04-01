pub mod extensions;
pub mod heuristics;
#[cfg(feature = "ai")]
pub mod ai;

use crate::scanner::Manifest;
use anyhow::{bail, Result};

/// Every language uc knows how to compile.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Language {
    C,
    Cpp,
    CSharp,
    Java,
    Rust,
    Go,
    Python,
    TypeScript,
    JavaScript,
    Kotlin,
    Swift,
    Zig,
}

impl Language {
    pub fn label(&self) -> &'static str {
        match self {
            Language::C => "C",
            Language::Cpp => "C++",
            Language::CSharp => "C#",
            Language::Java => "Java",
            Language::Rust => "Rust",
            Language::Go => "Go",
            Language::Python => "Python",
            Language::TypeScript => "TypeScript",
            Language::JavaScript => "JavaScript",
            Language::Kotlin => "Kotlin",
            Language::Swift => "Swift",
            Language::Zig => "Zig",
        }
    }

    /// Return the canonical short id used in --lang overrides
    pub fn from_id(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "c" => Some(Language::C),
            "cpp" | "c++" | "cxx" => Some(Language::Cpp),
            "csharp" | "cs" | "c#" => Some(Language::CSharp),
            "java" => Some(Language::Java),
            "rust" | "rs" => Some(Language::Rust),
            "go" | "golang" => Some(Language::Go),
            "python" | "py" => Some(Language::Python),
            "typescript" | "ts" => Some(Language::TypeScript),
            "javascript" | "js" => Some(Language::JavaScript),
            "kotlin" | "kt" => Some(Language::Kotlin),
            "swift" => Some(Language::Swift),
            "zig" => Some(Language::Zig),
            _ => None,
        }
    }
}

/// Result of language detection.
#[derive(Debug)]
pub struct Detection {
    pub language: Language,
    /// Tier that produced this result
    pub tier: DetectionTier,
    /// Human-readable notes about confidence / ambiguity
    pub confidence_notes: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DetectionTier {
    /// Determined from file extensions alone
    Extension,
    /// Determined from config files or import patterns
    Heuristic,
    /// Determined by calling Claude API
    Ai,
    /// User-supplied override
    Override,
}

/// Main entry — run tiers in order, stop at first confident answer.
pub fn detect(manifest: &Manifest) -> Result<Detection> {
    // Tier 1: config files (most reliable — CMakeLists.txt, pom.xml, Cargo.toml etc.)
    if let Some(d) = heuristics::detect_from_config(manifest) {
        return Ok(d);
    }

    // Tier 2: extension voting
    if let Some(d) = extensions::detect_from_extensions(manifest) {
        return Ok(d);
    }

    // Tier 3: import/include heuristics
    if let Some(d) = heuristics::detect_from_imports(manifest) {
        return Ok(d);
    }

    // Tier 4: AI fallback (only compiled with `--features ai`)
    #[cfg(feature = "ai")]
    {
        eprintln!(
            "  {} Static analysis inconclusive — falling back to AI detection.",
            "·".yellow()
        );
        return ai::detect_with_ai(manifest);
    }

    bail!(
        "Could not determine project language from {} source files. \
         Try `--lang <language>` to specify it explicitly.",
        manifest.source_files.len()
    )
}

/// Called when the user passes `--lang`
pub fn detect_override(lang: &str) -> Result<Detection> {
    match Language::from_id(lang) {
        Some(language) => Ok(Detection {
            language,
            tier: DetectionTier::Override,
            confidence_notes: vec![format!("Language overridden by --lang {lang}")],
        }),
        None => bail!(
            "Unknown language '{}'. \
             Supported values: c, cpp, csharp, java, rust, go, python, typescript, javascript, kotlin, swift, zig",
            lang
        ),
    }
}
