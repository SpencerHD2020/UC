use crate::planner::{BuildPlan, PostBuildStep};
use anyhow::Result;
use colored::Colorize;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

/// Result of running a compiler.
pub struct ExecutionResult {
    pub success: bool,
    pub error_count: usize,
    pub warning_count: usize,
}

pub fn execute(plan: &BuildPlan, verbose: bool) -> Result<ExecutionResult> {
    let mut cmd = build_command(plan);

    if verbose {
        println!(
            "   {} {} {}",
            "$".dimmed(),
            plan.toolchain.dimmed(),
            plan.flags.join(" ").dimmed()
        );
    }

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to spawn '{}': {}. Is it installed and on PATH?",
                plan.toolchain,
                e
            )
        })?;

    let mut error_count = 0usize;
    let mut warning_count = 0usize;

    // Stream stderr (where most compilers write diagnostics)
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let classified = classify_line(&line);
            match classified {
                DiagLevel::Error => {
                    error_count += 1;
                    eprintln!("   {}", line.red());
                }
                DiagLevel::Warning => {
                    warning_count += 1;
                    eprintln!("   {}", line.yellow());
                }
                DiagLevel::Note => {
                    eprintln!("   {}", line.cyan());
                }
                DiagLevel::Plain => {
                    if verbose {
                        eprintln!("   {}", line.dimmed());
                    }
                }
            }
        }
    }

    // Stream stdout if verbose
    if verbose {
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                println!("   {}", line.dimmed());
            }
        }
    }

    let status = child.wait()?;
    let success = status.success();

    // Run post-build step (e.g. jar packaging)
    if success {
        if let Some(post) = &plan.post_build {
            run_post_build(post, &plan.cwd, verbose)?;
        }
    }

    Ok(ExecutionResult {
        success,
        error_count,
        warning_count,
    })
}

fn build_command(plan: &BuildPlan) -> Command {
    // Split toolchain string in case it contains spaces (e.g. "npx tsc")
    let parts: Vec<&str> = plan.toolchain.splitn(2, ' ').collect();
    let mut cmd = Command::new(parts[0]);

    if parts.len() > 1 {
        cmd.arg(parts[1]);
    }

    // Add flags
    cmd.args(&plan.flags);

    // Add source files (for compilers that take them positionally)
    // Only if the toolchain is a direct compiler (not a build system)
    let is_build_system = matches!(
        plan.toolchain.as_str(),
        "cargo" | "dotnet" | "mvn" | "gradle" | "go" | "zig"
            | "./mvnw" | "./gradlew" | ".\\gradlew.bat"
    );

    if !is_build_system && !plan.sources.is_empty() {
        cmd.args(plan.sources.iter().map(|p| p.display().to_string()));
    }

    cmd.current_dir(&plan.cwd);

    // Set environment variables
    for (k, v) in &plan.env {
        cmd.env(k, v);
    }

    cmd
}

fn run_post_build(step: &PostBuildStep, cwd: &Path, verbose: bool) -> Result<()> {
    if verbose {
        println!(
            "   {} {} {}",
            "post-build:".dimmed(),
            step.command.dimmed(),
            step.args.join(" ").dimmed()
        );
    }

    let status = Command::new(&step.command)
        .args(&step.args)
        .current_dir(cwd)
        .status()
        .map_err(|e| anyhow::anyhow!("Post-build step '{}' failed to start: {}", step.command, e))?;

    if !status.success() {
        anyhow::bail!("Post-build step '{}' exited with error", step.command);
    }

    Ok(())
}

/// Run a compiled artifact (for `uc run`)
pub fn run_artifact(artifact: &Path, args: &[String]) -> Result<()> {
    let status = Command::new(artifact)
        .args(args)
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to run '{}': {}", artifact.display(), e))?;

    if !status.success() {
        let code = status.code().unwrap_or(1);
        anyhow::bail!("Program exited with code {code}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Diagnostic classification
// ---------------------------------------------------------------------------

enum DiagLevel {
    Error,
    Warning,
    Note,
    Plain,
}

fn classify_line(line: &str) -> DiagLevel {
    let lower = line.to_lowercase();

    // GCC/Clang style: "file.cpp:10:5: error: ..."
    // MSVC style: "file.cpp(10): error C2065: ..."
    // Java style: "error: ..."
    // Rust style: "error[E0001]: ..."
    if lower.contains(": error") || lower.starts_with("error") || lower.contains("error:") {
        return DiagLevel::Error;
    }
    if lower.contains(": warning") || lower.starts_with("warning") || lower.contains("warning:") {
        return DiagLevel::Warning;
    }
    if lower.contains(": note") || lower.starts_with("note") || lower.contains("note:") {
        return DiagLevel::Note;
    }

    DiagLevel::Plain
}
