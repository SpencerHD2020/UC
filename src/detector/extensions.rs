use super::{Detection, DetectionTier, Language};
use crate::scanner::Manifest;
use std::collections::HashMap;

/// Vote on language from file extensions.
/// Returns None if votes are too close to call confidently.
pub fn detect_from_extensions(manifest: &Manifest) -> Option<Detection> {
    let mut votes: HashMap<Language, usize> = HashMap::new();

    for path in &manifest.source_files {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if let Some(lang) = ext_to_language(&ext) {
            *votes.entry(lang).or_insert(0) += 1;
        }
    }

    if votes.is_empty() {
        return None;
    }

    // Sort by vote count descending
    let mut ranked: Vec<(Language, usize)> = votes.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));

    let (top_lang, top_count) = ranked.remove(0);
    let total: usize = top_count + ranked.iter().map(|(_, c)| c).sum::<usize>();

    // Require at least 60% of source files to agree
    let confidence = top_count as f64 / total as f64;

    let mut notes = vec![format!(
        "{}/{} files are {} ({:.0}% confidence)",
        top_count,
        total,
        top_lang.label(),
        confidence * 100.0
    )];

    if !ranked.is_empty() {
        let others: Vec<String> = ranked
            .iter()
            .take(3)
            .map(|(l, c)| format!("{} ({})", l.label(), c))
            .collect();
        notes.push(format!("Also detected: {}", others.join(", ")));
    }

    if confidence < 0.60 {
        // Too mixed — let heuristics try
        return None;
    }

    // Disambiguate C vs C++: if there are any .cpp/.cc/.cxx files, call it C++
    let top_lang = if top_lang == Language::C {
        disambiguate_c_cpp(manifest).unwrap_or(top_lang)
    } else {
        top_lang
    };

    Some(Detection {
        language: top_lang,
        tier: DetectionTier::Extension,
        confidence_notes: notes,
    })
}

fn ext_to_language(ext: &str) -> Option<Language> {
    match ext {
        "c" | "h" => Some(Language::C),
        "cc" | "cpp" | "cxx" | "c++" | "hh" | "hpp" | "hxx" => Some(Language::Cpp),
        "cs" => Some(Language::CSharp),
        "java" => Some(Language::Java),
        "rs" => Some(Language::Rust),
        "go" => Some(Language::Go),
        "py" | "pyw" => Some(Language::Python),
        "ts" | "mts" => Some(Language::TypeScript),
        "js" | "mjs" | "cjs" => Some(Language::JavaScript),
        "kt" | "kts" => Some(Language::Kotlin),
        "swift" => Some(Language::Swift),
        "zig" => Some(Language::Zig),
        _ => None,
    }
}

/// If we have any C++ extensions, prefer C++ over C.
fn disambiguate_c_cpp(manifest: &Manifest) -> Option<Language> {
    let cpp_exts = ["cc", "cpp", "cxx", "c++", "hh", "hpp", "hxx"];
    for path in &manifest.source_files {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if cpp_exts.contains(&ext.as_str()) {
            return Some(Language::Cpp);
        }
    }
    None
}
