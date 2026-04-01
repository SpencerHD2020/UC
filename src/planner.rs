use crate::detector::{Detection, Language};
use crate::resolver::DependencySet;
use crate::scanner::Manifest;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// A fully resolved, ready-to-execute build plan.
#[derive(Debug)]
pub struct BuildPlan {
    /// Compiler / toolchain binary (e.g. "cl.exe", "javac", "dotnet")
    pub toolchain: String,
    /// Primary entry point / source root
    pub entry_point: PathBuf,
    /// All source files to compile
    pub sources: Vec<PathBuf>,
    /// Final output artifact path
    pub output_artifact: PathBuf,
    /// All compiler flags (includes, libs, optimisation, …)
    pub flags: Vec<String>,
    /// Extra environment variables to set when invoking the toolchain
    pub env: Vec<(String, String)>,
    /// Working directory for the compiler process
    pub cwd: PathBuf,
    /// Language-specific post-build command (e.g. jar packaging)
    pub post_build: Option<PostBuildStep>,
}

#[derive(Debug)]
pub struct PostBuildStep {
    pub command: String,
    pub args: Vec<String>,
}

pub fn plan(
    root: &Path,
    output_dir: &Path,
    detection: &Detection,
    deps: &DependencySet,
    manifest: &Manifest,
) -> Result<BuildPlan> {
    std::fs::create_dir_all(output_dir).context("Failed to create output directory")?;

    match detection.language {
        Language::C => plan_c(root, output_dir, deps, manifest),
        Language::Cpp => plan_cpp(root, output_dir, deps, manifest),
        Language::CSharp => plan_csharp(root, output_dir, deps, manifest),
        Language::Java => plan_java(root, output_dir, deps, manifest),
        Language::Kotlin => plan_kotlin(root, output_dir, deps, manifest),
        Language::Rust => plan_rust(root, output_dir),
        Language::Go => plan_go(root, output_dir, manifest),
        Language::Python => plan_python(root, manifest),
        Language::TypeScript => plan_typescript(root, output_dir, manifest),
        Language::JavaScript => plan_javascript(root, manifest),
        Language::Swift => plan_swift(root, output_dir, manifest),
        Language::Zig => plan_zig(root, output_dir, manifest),
    }
}

// ---------------------------------------------------------------------------
// C
// ---------------------------------------------------------------------------

fn plan_c(root: &Path, out: &Path, deps: &DependencySet, manifest: &Manifest) -> Result<BuildPlan> {
    let compiler = find_c_compiler()?;
    let exe_name = exe_name(root, out);

    let sources: Vec<PathBuf> = manifest
        .source_files
        .iter()
        .filter(|f| matches_ext(f, &["c"]))
        .cloned()
        .collect();

    if sources.is_empty() {
        bail!("No .c files found");
    }

    let mut flags = vec!["-O2".into(), "-Wall".into(), "-Wextra".into()];
    flags.extend(deps.resolved.iter().cloned());
    flags.push(format!("-o{}", exe_name.display()));

    Ok(BuildPlan {
        toolchain: compiler,
        entry_point: find_entry_c(&sources),
        sources,
        output_artifact: exe_name,
        flags,
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

// ---------------------------------------------------------------------------
// C++
// ---------------------------------------------------------------------------

fn plan_cpp(root: &Path, out: &Path, deps: &DependencySet, manifest: &Manifest) -> Result<BuildPlan> {
    let compiler = find_cpp_compiler()?;
    let exe_name = exe_name(root, out);

    let cpp_exts = ["cc", "cpp", "cxx", "c++"];
    let sources: Vec<PathBuf> = manifest
        .source_files
        .iter()
        .filter(|f| matches_ext(f, &cpp_exts))
        .cloned()
        .collect();

    if sources.is_empty() {
        bail!("No C++ source files found");
    }

    let mut flags = Vec::new();

    // MSVC vs GCC/Clang flag styles
    if compiler.ends_with("cl.exe") || compiler == "cl" {
        flags.extend([
            "/EHsc".into(),
            "/std:c++17".into(),
            "/O2".into(),
            "/W3".into(),
            format!("/Fe:{}", exe_name.display()),
        ]);
    } else {
        flags.extend([
            "-std=c++17".into(),
            "-O2".into(),
            "-Wall".into(),
            "-Wextra".into(),
            format!("-o{}", exe_name.display()),
        ]);
    }

    flags.extend(deps.resolved.iter().cloned());
    for lib in &deps.link_libs {
        flags.push(format!("-l{lib}"));
    }
    for path in &deps.lib_paths {
        flags.push(format!("-L{path}"));
    }

    Ok(BuildPlan {
        toolchain: compiler,
        entry_point: find_entry_cpp(&sources),
        sources,
        output_artifact: exe_name,
        flags,
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

// ---------------------------------------------------------------------------
// C#
// ---------------------------------------------------------------------------

fn plan_csharp(root: &Path, out: &Path, deps: &DependencySet, manifest: &Manifest) -> Result<BuildPlan> {
    // Prefer `dotnet build` (handles NuGet automatically)
    if which("dotnet").is_some() {
        let csproj = manifest
            .config_files
            .iter()
            .find(|f| f.extension().map(|e| e == "csproj").unwrap_or(false));

        let entry = csproj
            .cloned()
            .unwrap_or_else(|| root.to_path_buf());

        return Ok(BuildPlan {
            toolchain: "dotnet".into(),
            entry_point: entry.clone(),
            sources: manifest.source_files.clone(),
            output_artifact: out.join("app.dll"),
            flags: vec!["build".into(), entry.display().to_string(),
                        "--output".into(), out.display().to_string()],
            env: vec![],
            cwd: root.to_path_buf(),
            post_build: None,
        });
    }

    // Fall back to csc / Roslyn
    let dotnet = find_dotnet()?;
    let compiler = which("csc")
        .or_else(|| which("mcs"))
        .unwrap_or(dotnet);


    let sources: Vec<PathBuf> = manifest
        .source_files
        .iter()
        .filter(|f| matches_ext(f, &["cs"]))
        .cloned()
        .collect();

    let exe_name = exe_name(root, out);
    let mut flags = vec![
        "/optimize+".into(),
        format!("/out:{}", exe_name.display()),
        "/target:exe".into(),
    ];
    flags.extend(deps.extra_flags.iter().cloned());

    Ok(BuildPlan {
        toolchain: compiler,
        entry_point: sources.first().cloned().unwrap_or_else(|| root.to_path_buf()),
        sources,
        output_artifact: exe_name,
        flags,
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

// ---------------------------------------------------------------------------
// Java
// ---------------------------------------------------------------------------

fn plan_java(root: &Path, out: &Path, deps: &DependencySet, manifest: &Manifest) -> Result<BuildPlan> {
    // If Maven or Gradle is present, delegate to them
    if manifest.config_files.iter().any(|f| f.file_name().map(|n| n == "pom.xml").unwrap_or(false)) {
        return plan_maven(root, out);
    }
    if manifest.config_files.iter().any(|f| {
        let n = f.file_name().and_then(|n| n.to_str()).unwrap_or("");
        n == "build.gradle" || n == "build.gradle.kts"
    }) {
        return plan_gradle(root, out);
    }

    let javac = find_javac()?;

    let sources: Vec<PathBuf> = manifest
        .source_files
        .iter()
        .filter(|f| matches_ext(f, &["java"]))
        .cloned()
        .collect();

    std::fs::create_dir_all(out)?;

    let mut flags = vec!["-d".into(), out.display().to_string()];

    if !deps.classpath.is_empty() {
        flags.push("-cp".into());
        flags.push(deps.classpath.join(if cfg!(windows) { ";" } else { ":" }));
    }

    // Find main class (file containing `public static void main`)
    let main_class = find_java_main_class(&sources, root);

    // Post-build: create an executable JAR
    let jar_name = out.join(format!(
        "{}.jar",
        root.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("app")
    ));

    let post_build = main_class.as_ref().map(|mc| PostBuildStep {
        command: "jar".into(),
        args: vec![
            "--create".into(),
            "--file".into(),
            jar_name.display().to_string(),
            "--main-class".into(),
            mc.clone(),
            "-C".into(),
            out.display().to_string(),
            ".".into(),
        ],
    });

    Ok(BuildPlan {
        toolchain: javac,
        entry_point: sources.first().cloned().unwrap_or_else(|| root.to_path_buf()),
        sources,
        output_artifact: jar_name,
        flags,
        env: vec![],
        cwd: root.to_path_buf(),
        post_build,
    })
}

fn plan_maven(root: &Path, _out: &Path) -> Result<BuildPlan> {
    let mvn = if root.join("mvnw").exists() || root.join("mvnw.cmd").exists() {
        if cfg!(windows) { ".\\mvnw.cmd".into() } else { "./mvnw".into() }
    } else {
        find_maven()?
    };

    Ok(BuildPlan {
        toolchain: mvn,
        entry_point: root.join("pom.xml"),
        sources: vec![],
        output_artifact: root.join("target"),
        flags: vec!["package".into(), "-DskipTests".into()],
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

fn plan_gradle(root: &Path, _out: &Path) -> Result<BuildPlan> {
    let gradle = if root.join("gradlew").exists() || root.join("gradlew.bat").exists() {
        if cfg!(windows) { ".\\gradlew.bat".into() } else { "./gradlew".into() }
    } else {
        find_gradle()?
    };

    Ok(BuildPlan {
        toolchain: gradle,
        entry_point: root.join("build.gradle"),
        sources: vec![],
        output_artifact: root.join("build").join("libs"),
        flags: vec!["build".into()],
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

fn plan_kotlin(root: &Path, out: &Path, _deps: &DependencySet, manifest: &Manifest) -> Result<BuildPlan> {
    // Gradle is the standard for Kotlin projects
    if manifest.config_files.iter().any(|f| {
        let n = f.file_name().and_then(|n| n.to_str()).unwrap_or("");
        n == "build.gradle.kts" || n == "build.gradle"
    }) {
        return plan_gradle(root, out);
    }

    let kotlinc = which("kotlinc").ok_or_else(|| anyhow::anyhow!("'kotlinc' not found on PATH"))?;
    let jar = out.join(format!("{}.jar", root.file_name().and_then(|n| n.to_str()).unwrap_or("app")));

    Ok(BuildPlan {
        toolchain: kotlinc,
        entry_point: manifest.source_files.first().cloned().unwrap_or_else(|| root.to_path_buf()),
        sources: manifest.source_files.clone(),
        output_artifact: jar.clone(),
        flags: vec!["-include-runtime".into(), "-d".into(), jar.display().to_string()],
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

// ---------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------

fn plan_rust(root: &Path, _out: &Path) -> Result<BuildPlan> {
    let cargo = which("cargo").ok_or_else(|| anyhow::anyhow!("'cargo' not found on PATH"))?;

    Ok(BuildPlan {
        toolchain: cargo,
        entry_point: root.join("Cargo.toml"),
        sources: vec![],
        output_artifact: root.join("target").join("release"),
        flags: vec!["build".into(), "--release".into()],
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

fn plan_go(root: &Path, out: &Path, _manifest: &Manifest) -> Result<BuildPlan> {
    let go = which("go").ok_or_else(|| anyhow::anyhow!("'go' not found on PATH"))?;
    let exe = exe_name(root, out);

    Ok(BuildPlan {
        toolchain: go,
        entry_point: root.to_path_buf(),
        sources: vec![],
        output_artifact: exe.clone(),
        flags: vec!["build".into(), "-o".into(), exe.display().to_string(), "./...".into()],
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

fn plan_python(root: &Path, manifest: &Manifest) -> Result<BuildPlan> {
    let python = which("python3")
        .or_else(|| which("python"))
        .ok_or_else(|| anyhow::anyhow!("'python3' not found on PATH"))?;

    // Find main entry point
    let entry = find_python_main(manifest, root)
        .unwrap_or_else(|| manifest.source_files.first().cloned().unwrap_or_else(|| root.to_path_buf()));

    Ok(BuildPlan {
        toolchain: python,
        entry_point: entry.clone(),
        sources: manifest.source_files.clone(),
        output_artifact: entry.clone(),
        flags: vec![entry.display().to_string()],
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

// ---------------------------------------------------------------------------
// TypeScript
// ---------------------------------------------------------------------------

fn plan_typescript(root: &Path, out: &Path, manifest: &Manifest) -> Result<BuildPlan> {
    // Prefer npx tsc
    let tsc = which("tsc")
        .map(|_| "tsc".into())
        .unwrap_or_else(|| "npx tsc".into());

    Ok(BuildPlan {
        toolchain: tsc,
        entry_point: root.join("tsconfig.json"),
        sources: manifest.source_files.clone(),
        output_artifact: out.to_path_buf(),
        flags: vec!["--outDir".into(), out.display().to_string()],
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

// ---------------------------------------------------------------------------
// JavaScript
// ---------------------------------------------------------------------------

fn plan_javascript(root: &Path, manifest: &Manifest) -> Result<BuildPlan> {
    let node = which("node").ok_or_else(|| anyhow::anyhow!("'node' not found on PATH"))?;

    let entry = find_js_main(manifest, root)
        .unwrap_or_else(|| manifest.source_files.first().cloned().unwrap_or(root.to_path_buf()));

    Ok(BuildPlan {
        toolchain: node,
        entry_point: entry.clone(),
        sources: manifest.source_files.clone(),
        output_artifact: entry.clone(),
        flags: vec![entry.display().to_string()],
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

// ---------------------------------------------------------------------------
// Swift
// ---------------------------------------------------------------------------

fn plan_swift(root: &Path, out: &Path, manifest: &Manifest) -> Result<BuildPlan> {
    let swiftc = which("swiftc").ok_or_else(|| anyhow::anyhow!("'swiftc' not found on PATH"))?;
    let exe = exe_name(root, out);

    let sources: Vec<PathBuf> = manifest
        .source_files
        .iter()
        .filter(|f| matches_ext(f, &["swift"]))
        .cloned()
        .collect();

    Ok(BuildPlan {
        toolchain: swiftc,
        entry_point: sources.first().cloned().unwrap_or_else(|| root.to_path_buf()),
        sources: sources.clone(),
        output_artifact: exe.clone(),
        flags: vec!["-O".into(), "-o".into(), exe.display().to_string()],
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

// ---------------------------------------------------------------------------
// Zig
// ---------------------------------------------------------------------------

fn plan_zig(root: &Path, out: &Path, _manifest: &Manifest) -> Result<BuildPlan> {
    let zig = which("zig").ok_or_else(|| anyhow::anyhow!("'zig' not found on PATH"))?;

    Ok(BuildPlan {
        toolchain: zig,
        entry_point: root.join("build.zig"),
        sources: vec![],
        output_artifact: out.to_path_buf(),
        flags: vec!["build".into(), "--prefix".into(), out.display().to_string()],
        env: vec![],
        cwd: root.to_path_buf(),
        post_build: None,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn exe_name(root: &Path, out: &Path) -> PathBuf {
    let stem = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("app");
    if cfg!(windows) {
        out.join(format!("{stem}.exe"))
    } else {
        out.join(stem)
    }
}

fn matches_ext(path: &Path, exts: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| exts.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn which(name: &str) -> Option<String> {
    let output = std::process::Command::new(if cfg!(windows) { "where" } else { "which" })
        .arg(name)
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    }
}

/// Attempt to find a C++ compiler on Visual Studio installation on Windows.
fn find_vs_cpp_compiler() -> Option<String> {
    #[cfg(windows)]
    {
        // Try common Visual Studio paths (2022, 2019)
        let vs_paths = vec![
            r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC",
            r"C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC",
            r"C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Tools\MSVC",
            r"C:\Program Files (x86)\Microsoft Visual Studio\2019\Community\VC\Tools\MSVC",
            r"C:\Program Files (x86)\Microsoft Visual Studio\2019\Professional\VC\Tools\MSVC",
            r"C:\Program Files (x86)\Microsoft Visual Studio\2019\Enterprise\VC\Tools\MSVC",
        ];

        for vs_path in vs_paths {
            let path = PathBuf::from(vs_path);
            if let Ok(entries) = std::fs::read_dir(&path) {
                // Find the latest MSVC version directory
                if let Some(latest) = entries
                    .flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.path())
                    .max()
                {
                    let cl_path = latest.join("bin").join("Hostx64").join("x64").join("cl.exe");
                    if cl_path.exists() {
                        return Some(cl_path.to_string_lossy().to_string());
                    }
                    // Fallback: try Hostx86
                    let cl_path = latest.join("bin").join("Hostx86").join("x86").join("cl.exe");
                    if cl_path.exists() {
                        return Some(cl_path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    None
}

/// Attempt to find MinGW or other GCC installations.
fn find_gcc_cpp_compiler() -> Option<String> {
    // Check common MinGW paths on Windows
    #[cfg(windows)]
    {
        let minigw_paths = vec![
            r"C:\Program Files\mingw-w64",
            r"C:\Program Files (x86)\mingw-w64",
            r"C:\mingw-w64",
            r"C:\mingw",
        ];

        for base_path in minigw_paths {
            let path = PathBuf::from(base_path);
            if let Ok(entries) = std::fs::read_dir(&path) {
                for entry in entries.flatten() {
                    let bin_path = entry.path().join("bin").join("g++.exe");
                    if bin_path.exists() {
                        return Some(bin_path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    // Try PATH as fallback
    which("g++")
}

/// Attempt to find Clang/LLVM installations.
fn find_clang_cpp_compiler() -> Option<String> {
    #[cfg(windows)]
    {
        let llvm_paths = vec![
            r"C:\Program Files\LLVM\bin\clang++.exe",
            r"C:\Program Files (x86)\LLVM\bin\clang++.exe",
        ];

        for path in llvm_paths {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p.to_string_lossy().to_string());
            }
        }
    }

    // Try PATH
    which("clang++")
}

/// Attempt to find .NET SDK.
fn find_dotnet() -> Result<String> {
    if let Some(dotnet) = which("dotnet") {
        return Ok(dotnet);
    }

    #[cfg(windows)]
    {
        let dotnet_paths = vec![
            r"C:\Program Files\dotnet\dotnet.exe",
            r"C:\Program Files (x86)\dotnet\dotnet.exe",
        ];
        for path in dotnet_paths {
            let p = PathBuf::from(path);
            if p.exists() {
                return Ok(p.to_string_lossy().to_string());
            }
        }
    }

    bail!(
        "No .NET SDK found! To build C# projects, install .NET SDK:\n\n\
         1. Visit: https://dotnet.microsoft.com/download\n\
         2. Download the latest .NET SDK (not just Runtime)\n\
         3. Run the installer and follow the prompts\n\
         4. Restart your terminal after installation\n\n\
         Alternatively, you can:\n\
         - Use Visual Studio (includes .NET SDK): https://visualstudio.microsoft.com/\n\
         - Use Visual Studio Code with C# extension\n\n\
         Verify installation by running: dotnet --version"
    )
}

fn find_javac() -> Result<String> {
    if let Some(javac) = which("javac") {
        return Ok(javac);
    }

    #[cfg(windows)]
    {
        let java_paths = vec![
            r"C:\Program Files\Java",
            r"C:\Program Files (x86)\Java",
        ];
        for base in java_paths {
            if let Ok(entries) = std::fs::read_dir(base) {
                for entry in entries.flatten() {
                    let javac_path = entry.path().join("bin").join("javac.exe");
                    if javac_path.exists() {
                        return Ok(javac_path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    bail!(
        "No Java Development Kit (JDK) found! To build Java projects:\n\n\
         1. Download the latest JDK from:\n\
            - Oracle JDK: https://www.oracle.com/java/technologies/downloads/\n\
            - Eclipse Temurin (free): https://adoptium.net/\n\
            - Amazon Corretto: https://aws.amazon.com/corretto/\n\
            - OpenJDK: https://openjdk.java.net/\n\n\
         2. Install the JDK (NOT just JRE - you need development tools)\n\
         3. The installer should add JDK/bin to your PATH\n\
         4. Restart your terminal after installation\n\n\
         Verify installation by running: javac -version"
    )
}

fn find_maven() -> Result<String> {
    if let Some(mvn) = which("mvn") {
        return Ok(mvn);
    }

    bail!(
        "Maven not found on PATH. To use Maven:\n\n\
         1. Download from: https://maven.apache.org/download.cgi\n\
         2. Extract and add the bin/ folder to your PATH\n\
         3. Restart your terminal\n\n\
         Or install via package manager:\n\
         - Windows (Chocolatey): choco install maven\n\
         - Linux (Ubuntu): sudo apt install maven\n\
         - macOS (Homebrew): brew install maven\n\n\
         Verify with: mvn -version"
    )
}

fn find_gradle() -> Result<String> {
    if let Some(gradle) = which("gradle") {
        return Ok(gradle);
    }

    bail!(
        "Gradle not found on PATH. To use Gradle:\n\n\
         1. Download from: https://gradle.org/releases/\n\
         2. Extract and add the bin/ folder to your PATH\n\
         3. Restart your terminal\n\n\
         Or install via package manager:\n\
         - Windows (Chocolatey): choco install gradle\n\
         - Linux (Ubuntu): sudo apt install gradle\n\
         - macOS (Homebrew): brew install gradle\n\n\
         Verify with: gradle --version"
    )
}

fn find_c_compiler() -> Result<String> {
    // Windows: prefer cl.exe; fallback to gcc/clang
    if cfg!(windows) {
        // Try on PATH first
        if let Some(cl) = which("cl") { return Ok(cl); }
        
        // Try Visual Studio installation
        if let Some(cl) = find_vs_cpp_compiler() {
            return Ok(cl.replace("++", "").replace("clang", "cl"));
        }
        
        // Try GCC/MinGW
        if let Some(gcc) = find_gcc_cpp_compiler() {
            return Ok(gcc.replace("++", ""));
        }
        
        // Try Clang/LLVM
        if let Some(clang) = find_clang_cpp_compiler() {
            return Ok(clang.replace("++", ""));
        }
    } else {
        if let Some(gcc) = which("gcc") { return Ok(gcc); }
        if let Some(clang) = which("clang") { return Ok(clang); }
    }

    let error_msg = if cfg!(windows) {
        "No C compiler found! The following options are available:\n\n\
         1. **Visual Studio** (recommended for C++ development):\n\
            - Download: https://visualstudio.microsoft.com/downloads/\n\
            - Choose \"Desktop development with C++\"\n\
            - Install and reopen this terminal\n\n\
         2. **MinGW-w64** (lightweight alternative):\n\
            - Download: https://www.mingw-w64.org/\n\
            - Installation instructions: https://www.mingw-w64.org/online-installer/\n\
            - Add the bin/ directory to your PATH\n\n\
         3. **LLVM/Clang**:\n\
            - Download: https://releases.llvm.org/\n\
            - Ensure it's in your PATH"
    } else {
        "No C compiler found! Please install one of the following:\n\n\
         Linux (Debian/Ubuntu): sudo apt-get install build-essential\n\
         Linux (Fedora/RHEL): sudo dnf install gcc gcc-c++\n\
         macOS: Install Xcode Command Line Tools: xcode-select --install"
    };

    bail!("{}", error_msg)
}

fn find_cpp_compiler() -> Result<String> {
    // Windows: prefer cl.exe; fallback to gcc/clang
    if cfg!(windows) {
        // Try on PATH first (common for cl.exe when VS is set up)
        if let Some(cl) = which("cl") { return Ok(cl); }
        if let Some(gpp) = which("g++") { return Ok(gpp); }
        if let Some(clangpp) = which("clang++") { return Ok(clangpp); }
        
        // Try Visual Studio installation directories
        if let Some(cl) = find_vs_cpp_compiler() {
            return Ok(cl);
        }
        
        // Try GCC/MinGW
        if let Some(gpp) = find_gcc_cpp_compiler() {
            return Ok(gpp);
        }
        
        // Try Clang/LLVM
        if let Some(clangpp) = find_clang_cpp_compiler() {
            return Ok(clangpp);
        }
    } else {
        if let Some(gpp) = which("g++") { return Ok(gpp); }
        if let Some(clangpp) = which("clang++") { return Ok(clangpp); }
    }

    let error_msg = if cfg!(windows) {
        "No C++ compiler found! The following options are available:\n\n\
         1. **Visual Studio** (recommended for C/C++ development in Qt):\n\
            - Download: https://visualstudio.microsoft.com/downloads/\n\
            - During installation, select \"Desktop development with C++\"\n\
            - This gives you the MSVC compiler (cl.exe) + Qt integration\n\n\
         2. **MinGW-w64** (lightweight GCC alternative):\n\
            - Download: https://www.mingw-w64.org/\n\
            - Installation guide: https://www.mingw-w64.org/online-installer/\n\
            - After installation, add the bin/ directory to your PATH\n\
            - Restart your terminal after PATH changes\n\n\
         3. **LLVM/Clang**:\n\
            - Download: https://releases.llvm.org/download.html\n\
            - Ensure installation directory is added to your PATH\n\n\
         For Qt development specifically:\n\
            - Consider using Qt Creator which bundles MinGW: https://www.qt.io/download\n\
            - Or Visual Studio with Qt Tools extension"
    } else {
        "No C++ compiler found! Install one of the following:\n\n\
         **Linux (Debian/Ubuntu):**\n\
            sudo apt-get update\n\
            sudo apt-get install build-essential g++ gcc\n\n\
         **Linux (Fedora/RHEL):**\n\
            sudo dnf install gcc gcc-c++ make\n\n\
         **macOS:**\n\
            xcode-select --install\n\
            # Or install from: https://developer.apple.com/download/\n\n\
         **Note:** After installation, you may need to restart your terminal."
    };

    bail!("{}", error_msg)
}

fn find_entry_c(sources: &[PathBuf]) -> PathBuf {
    find_file_with_main(sources, "int main").unwrap_or_else(|| sources[0].clone())
}

fn find_entry_cpp(sources: &[PathBuf]) -> PathBuf {
    find_file_with_main(sources, "int main").unwrap_or_else(|| sources[0].clone())
}

fn find_file_with_main(sources: &[PathBuf], pattern: &str) -> Option<PathBuf> {
    for src in sources {
        if let Ok(content) = std::fs::read_to_string(src) {
            if content.contains(pattern) {
                return Some(src.clone());
            }
        }
    }
    None
}

fn find_java_main_class(sources: &[PathBuf], root: &Path) -> Option<String> {
    for src in sources {
        if let Ok(content) = std::fs::read_to_string(src) {
            if content.contains("public static void main") {
                // Derive fully-qualified class name from path
                return java_fqcn(src, root);
            }
        }
    }
    None
}

fn java_fqcn(path: &Path, root: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let s = rel.to_string_lossy().replace('\\', "/");
    // Strip "src/main/java/" if present (standard Maven layout)
    let s = s
        .strip_prefix("src/main/java/")
        .unwrap_or(&s)
        .strip_suffix(".java")
        .unwrap_or(&s)
        .replace('/', ".");
    Some(s)
}

fn find_python_main(manifest: &Manifest, root: &Path) -> Option<PathBuf> {
    // Priority: __main__.py, main.py, app.py, run.py
    for candidate in &["__main__.py", "main.py", "app.py", "run.py"] {
        let p = root.join(candidate);
        if p.exists() {
            return Some(p);
        }
    }
    manifest.source_files.first().cloned()
}

fn find_js_main(manifest: &Manifest, root: &Path) -> Option<PathBuf> {
    // Check package.json for "main" field
    let pkg = root.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg) {
        for line in content.lines() {
            let t = line.trim();
            if t.starts_with(r#""main""#) || t.starts_with(r#""main" "#) {
                if let Some(start) = t.find('"').and_then(|i| t[i+1..].find('"').map(|j| i+j+2)) {
                    if let Some(end) = t[start..].find('"') {
                        let val = &t[start..start+end];
                        let p = root.join(val);
                        if p.exists() { return Some(p); }
                    }
                }
            }
        }
    }
    // Fallback: index.js or first source file
    let index = root.join("index.js");
    if index.exists() { return Some(index); }
    manifest.source_files.first().cloned()
}
