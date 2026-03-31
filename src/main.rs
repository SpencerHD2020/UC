mod cache;
mod detector;
mod executor;
mod planner;
mod resolver;
mod scanner;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;
use std::time::Instant;

/// uc — Universal Compiler
/// Zero-config build tool. Point it at a project; it figures out the rest.
#[derive(Parser)]
#[command(
    name = "uc",
    version,
    about = "Universal Compiler — zero-config build tool",
    long_about = None,
    arg_required_else_help = true,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Detect language(s) and compile the project
    Build {
        /// Project root (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output directory for compiled artifacts
        #[arg(short, long, default_value = "./out")]
        output: PathBuf,

        /// Force a full rebuild, ignoring incremental cache
        #[arg(long)]
        clean: bool,

        /// Show verbose compiler output
        #[arg(short, long)]
        verbose: bool,

        /// Override detected language (e.g. cpp, java, csharp)
        #[arg(long)]
        lang: Option<String>,
    },

    /// Compile and immediately run the output
    Run {
        /// Project root (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Arguments to pass to the compiled program
        #[arg(last = true)]
        args: Vec<String>,

        /// Show verbose compiler output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Remove all build artifacts and caches
    Clean {
        /// Project root (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Analyse a project without building it
    Inspect {
        /// Project root (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {}", "error:".red().bold(), e);
        // Walk the error chain
        let mut source = e.source();
        while let Some(cause) = source {
            eprintln!("  {} {}", "caused by:".dimmed(), cause);
            source = cause.source();
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Build {
            path,
            output,
            clean,
            verbose,
            lang,
        } => cmd_build(path, output, clean, verbose, lang),

        Command::Run { path, args, verbose } => cmd_run(path, args, verbose),

        Command::Clean { path } => cmd_clean(path),

        Command::Inspect { path } => cmd_inspect(path),
    }
}

// ---------------------------------------------------------------------------
// build
// ---------------------------------------------------------------------------

fn cmd_build(
    path: PathBuf,
    output: PathBuf,
    force_clean: bool,
    verbose: bool,
    lang_override: Option<String>,
) -> Result<()> {
    let start = Instant::now();
    let root = path.canonicalize()?;

    print_banner("build", &root);

    // 1. Scan
    print_step(1, "Scanning project files");
    let manifest = scanner::scan(&root)?;
    println!(
        "   {} source files across {} directories",
        manifest.source_files.len().to_string().cyan(),
        manifest.dir_count.to_string().cyan()
    );

    // 2. Detect language
    print_step(2, "Detecting language");
    let detection = if let Some(ref lang) = lang_override {
        detector::detect_override(lang)?
    } else {
        detector::detect(&manifest)?
    };
    println!("   Detected: {}", detection.language.label().green().bold());
    if !detection.confidence_notes.is_empty() {
        for note in &detection.confidence_notes {
            println!("   {} {}", "·".dimmed(), note.dimmed());
        }
    }

    // 3. Resolve dependencies
    print_step(3, "Resolving dependencies");
    let deps = resolver::resolve(&root, &detection, &manifest)?;
    if deps.resolved.is_empty() {
        println!("   No external dependencies detected");
    } else {
        for d in &deps.resolved {
            println!("   {} {}", "✓".green(), d);
        }
    }
    if !deps.missing.is_empty() {
        for d in &deps.missing {
            println!("   {} {} (not found on PATH or known registries)", "✗".red(), d);
        }
    }

    // 4. Load / validate cache
    if force_clean {
        print_step(4, "Cleaning previous build artifacts");
        cache::clear(&root)?;
    } else {
        print_step(4, "Checking incremental build cache");
    }
    let build_cache = cache::load(&root)?;
    let changed = cache::changed_files(&manifest, &build_cache);
    if !force_clean && changed.is_empty() {
        println!("   {} Nothing changed — already up to date.", "✓".green());
        return Ok(());
    }
    println!(
        "   {} file(s) need recompilation",
        changed.len().to_string().yellow()
    );

    // 5. Generate build plan
    print_step(5, "Generating build plan");
    let plan = planner::plan(&root, &output, &detection, &deps, &manifest)?;
    if verbose {
        println!("   Toolchain : {}", plan.toolchain.cyan());
        println!("   Entry     : {}", plan.entry_point.display().to_string().cyan());
        println!("   Flags     : {}", plan.flags.join(" ").cyan());
        println!("   Output    : {}", plan.output_artifact.display().to_string().cyan());
    } else {
        println!("   {} → {}", plan.toolchain.cyan(), plan.output_artifact.display().to_string().cyan());
    }

    // 6. Execute
    print_step(6, "Compiling");
    let result = executor::execute(&plan, verbose)?;

    // 7. Update cache
    cache::save(&root, &manifest)?;

    // Summary
    let elapsed = start.elapsed();
    if result.success {
        println!(
            "\n{} Build succeeded in {:.2}s",
            "✓".green().bold(),
            elapsed.as_secs_f64()
        );
        println!(
            "  Artifact: {}",
            plan.output_artifact.display().to_string().green()
        );
    } else {
        anyhow::bail!(
            "Build failed with {} error(s). See output above.",
            result.error_count
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

fn cmd_run(path: PathBuf, args: Vec<String>, verbose: bool) -> Result<()> {
    let root = path.canonicalize()?;
    let output = root.join("out");

    print_banner("run", &root);

    // Re-use build pipeline
    let manifest = scanner::scan(&root)?;
    let detection = detector::detect(&manifest)?;
    let deps = resolver::resolve(&root, &detection, &manifest)?;
    let plan = planner::plan(&root, &output, &detection, &deps, &manifest)?;
    let result = executor::execute(&plan, verbose)?;

    if !result.success {
        anyhow::bail!("Build failed — cannot run.");
    }

    println!("\n{} Running {}\n", "▶".cyan().bold(), plan.output_artifact.display());
    executor::run_artifact(&plan.output_artifact, &args)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// clean
// ---------------------------------------------------------------------------

fn cmd_clean(path: PathBuf) -> Result<()> {
    let root = path.canonicalize()?;
    print_banner("clean", &root);

    cache::clear(&root)?;

    let out = root.join("out");
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
        println!("  {} Removed {}", "✓".green(), out.display());
    }

    println!("\n{} Clean complete.", "✓".green().bold());
    Ok(())
}

// ---------------------------------------------------------------------------
// inspect
// ---------------------------------------------------------------------------

fn cmd_inspect(path: PathBuf) -> Result<()> {
    let root = path.canonicalize()?;
    print_banner("inspect", &root);

    let manifest = scanner::scan(&root)?;
    let detection = detector::detect(&manifest)?;
    let deps = resolver::resolve(&root, &detection, &manifest)?;

    println!("\n{}", "── Project Summary ─────────────────────────".dimmed());
    println!("  Root        : {}", root.display().to_string().cyan());
    println!("  Language    : {}", detection.language.label().green().bold());
    println!("  Source files: {}", manifest.source_files.len().to_string().cyan());
    println!("  Total files : {}", manifest.all_files.len().to_string().cyan());

    println!("\n{}", "── Source Files ────────────────────────────".dimmed());
    for f in &manifest.source_files {
        println!("  {}", f.display().to_string().dimmed());
    }

    println!("\n{}", "── Dependencies ────────────────────────────".dimmed());
    if deps.resolved.is_empty() && deps.missing.is_empty() {
        println!("  None detected");
    }
    for d in &deps.resolved {
        println!("  {} {}", "✓".green(), d);
    }
    for d in &deps.missing {
        println!("  {} {} (unresolved)", "✗".red(), d);
    }

    println!("\n{}", "── Detected Config Files ───────────────────".dimmed());
    for f in &manifest.config_files {
        println!("  {}", f.display().to_string().cyan());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Shared print helpers
// ---------------------------------------------------------------------------

fn print_banner(cmd: &str, root: &std::path::Path) {
    println!(
        "\n{} {} {}",
        "uc".cyan().bold(),
        cmd.bold(),
        root.display().to_string().dimmed()
    );
    println!("{}", "─".repeat(50).dimmed());
}

fn print_step(n: u8, label: &str) {
    println!("\n{} {}", format!("[{n}]").cyan(), label.bold());
}
