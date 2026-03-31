pub mod cpp;
pub mod csharp;
pub mod java;
pub mod generic;

use crate::detector::{Detection, Language};
use crate::scanner::Manifest;
use anyhow::Result;
use std::path::Path;

/// Result of dependency resolution for a project.
#[derive(Debug, Default)]
pub struct DependencySet {
    /// Include paths / classpath entries / module paths that were found
    pub resolved: Vec<String>,
    /// Dependencies that were referenced but could not be located
    pub missing: Vec<String>,
    /// Extra flags to pass verbatim to the compiler/linker
    pub extra_flags: Vec<String>,
    /// Classpath entries (Java)
    pub classpath: Vec<String>,
    /// Library search paths (-L flags for C/C++)
    pub lib_paths: Vec<String>,
    /// Libraries to link (-l flags for C/C++)
    pub link_libs: Vec<String>,
}

pub fn resolve(
    root: &Path,
    detection: &Detection,
    manifest: &Manifest,
) -> Result<DependencySet> {
    match detection.language {
        Language::C | Language::Cpp => cpp::resolve(root, manifest),
        Language::Java | Language::Kotlin => java::resolve(root, manifest),
        Language::CSharp => csharp::resolve(root, manifest),
        _ => generic::resolve(root, manifest),
    }
}
