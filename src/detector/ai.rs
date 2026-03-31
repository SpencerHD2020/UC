#![cfg(feature = "ai")]

use super::{Detection, DetectionTier, Language};
use crate::scanner::Manifest;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::env;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-opus-4-5";

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

pub fn detect_with_ai(manifest: &Manifest) -> Result<Detection> {
    let api_key = env::var("ANTHROPIC_API_KEY")
        .context("ANTHROPIC_API_KEY not set; required for AI language detection")?;

    // Build a compact project summary to send
    let file_list: Vec<String> = manifest
        .source_files
        .iter()
        .take(40)
        .map(|p| {
            p.strip_prefix(&manifest.root)
                .unwrap_or(p)
                .display()
                .to_string()
        })
        .collect();

    let config_list: Vec<String> = manifest
        .config_files
        .iter()
        .map(|p| {
            p.strip_prefix(&manifest.root)
                .unwrap_or(p)
                .display()
                .to_string()
        })
        .collect();

    let prompt = format!(
        "You are a build-system expert. Based on these project files, identify the primary \
         programming language. Reply with ONLY the language id from this list: \
         c, cpp, csharp, java, rust, go, python, typescript, javascript, kotlin, swift, zig\n\n\
         Config files: {}\n\
         Source files: {}",
        config_list.join(", "),
        file_list.join(", ")
    );

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&ApiRequest {
            model: MODEL.into(),
            max_tokens: 16,
            messages: vec![Message {
                role: "user".into(),
                content: prompt,
            }],
        })
        .send()
        .context("Failed to reach Anthropic API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        bail!("Anthropic API returned {status}: {body}");
    }

    let api_resp: ApiResponse = response.json().context("Failed to parse API response")?;

    let text = api_resp
        .content
        .iter()
        .find(|b| b.kind == "text")
        .and_then(|b| b.text.as_deref())
        .unwrap_or("")
        .trim()
        .to_lowercase();

    match Language::from_id(&text) {
        Some(lang) => Ok(Detection {
            language: lang,
            tier: DetectionTier::Ai,
            confidence_notes: vec![format!("AI identified language as '{text}'")],
        }),
        None => bail!("AI returned unrecognised language id '{text}'"),
    }
}
