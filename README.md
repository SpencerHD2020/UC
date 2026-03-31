# uc — Universal Compiler

Zero-config build tool. Point it at a project directory and it figures out the rest.

```
uc build ./my-project
uc run   ./my-project
uc clean ./my-project
uc inspect ./my-project
```

No `CMakeLists.txt`. No `pom.xml` (unless you already have one). No config files to write.

---

## Installation

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable, 1.75+)

### Build from source

```powershell
git clone https://github.com/you/uc
cd uc
cargo build --release
# Binary lands at: target\release\uc.exe
```

Add `target\release` to your `PATH`, or copy `uc.exe` anywhere on your `PATH`.

### Optional: AI fallback

If static analysis can't determine the language, `uc` can call Claude:

```powershell
cargo build --release --features ai
$env:ANTHROPIC_API_KEY = "sk-ant-..."
```

---

## Usage

### `uc build [PATH]`

Detects language, resolves dependencies, and compiles.

```
uc build .
uc build ./my-cpp-project --verbose
uc build ./my-java-project --output ./dist
uc build . --lang cpp          # override detection
uc build . --clean             # force full rebuild
```

**Options**

| Flag | Description |
|------|-------------|
| `--output <DIR>` | Output directory (default: `./out`) |
| `--clean` | Ignore incremental cache, rebuild everything |
| `--verbose` | Show full compiler command and output |
| `--lang <LANG>` | Override language detection |

### `uc run [PATH]`

Build then immediately execute the output.

```
uc run .
uc run ./my-project -- --port 8080   # args after -- go to the program
```

### `uc inspect [PATH]`

Analyse without building. Shows detected language, source files, and dependencies.

```
uc inspect ./unknown-project
```

### `uc clean [PATH]`

Remove `./out` and the incremental cache file (`.uc-cache.json`).

---

## Language Support

| Language | Detection | Toolchain | Notes |
|----------|-----------|-----------|-------|
| C | Extensions + includes | `cl` / `gcc` / `clang` | |
| C++ | Extensions + includes | `cl` / `g++` / `clang++` | |
| C# | `.csproj` / `.sln` | `dotnet` / `csc` | NuGet via `dotnet build` |
| Java | `pom.xml` / `build.gradle` / extensions | `javac` / `mvn` / `gradle` | Creates executable JAR |
| Kotlin | `build.gradle.kts` / extensions | `kotlinc` / `gradle` | |
| Rust | `Cargo.toml` | `cargo build --release` | |
| Go | `go.mod` / extensions | `go build` | |
| Python | Extensions / `pyproject.toml` | `python3` | Runs directly |
| TypeScript | `tsconfig.json` / extensions | `tsc` / `npx tsc` | |
| JavaScript | `package.json` / extensions | `node` | Runs directly |
| Swift | Extensions | `swiftc` | |
| Zig | `build.zig` / extensions | `zig build` | |

---

## How Detection Works

`uc` runs through four tiers and stops at the first confident result:

```
Tier 1 — Config files     pom.xml → Java, Cargo.toml → Rust, *.csproj → C#, …
Tier 2 — Extension vote   ≥60% of source files share an extension → that language
Tier 3 — Import analysis  #include <iostream> → C++, import java.util → Java, …
Tier 4 — AI (optional)    Calls Claude API with the file list; needs --features ai
```

You can always skip detection entirely with `--lang`.

---

## Incremental Builds

On each successful build, `uc` writes a `.uc-cache.json` file at the project root
containing SHA-256 hashes of every source file. On the next `uc build`, only changed
files are reported as needing recompilation.

> **Note:** For build systems like `cargo`, `dotnet`, `mvn`, and `gradle`, `uc`
> delegates incremental tracking to the native toolchain. The cache is still written
> but the full toolchain command is always invoked (they handle their own incrementality).

---

## Dependency Resolution

| Language | Strategy |
|----------|----------|
| C / C++ | Scans `#include` directives; checks project `include/`, vcpkg, Conan, MSVC SDK dirs |
| Java | Scans `import` statements; searches Maven local repo (`~/.m2`), local `lib/` |
| C# | Reads `<PackageReference>` from `.csproj`; checks NuGet global cache (`~/.nuget`) |
| Others | Detects lock files (`Cargo.lock`, `go.sum`, `package-lock.json`, …) |

---

## Project Structure

```
uc/
├── Cargo.toml
└── src/
    ├── main.rs            # CLI (clap) + command dispatch
    ├── scanner.rs         # Directory walk + file manifest
    ├── cache.rs           # SHA-256 incremental build cache
    ├── planner.rs         # Build plan generation per language
    ├── executor.rs        # Subprocess execution + log streaming
    ├── detector/
    │   ├── mod.rs         # Detection orchestrator (tiers 1-4)
    │   ├── extensions.rs  # Tier 2: extension voting
    │   ├── heuristics.rs  # Tiers 1+3: config files + import patterns
    │   └── ai.rs          # Tier 4: Claude API fallback (feature-gated)
    └── resolver/
        ├── mod.rs         # Resolver dispatch
        ├── cpp.rs         # C/C++ include + library resolution
        ├── java.rs        # Java JAR + Maven local repo
        ├── csharp.rs      # C# NuGet + .csproj parsing
        └── generic.rs     # Rust, Go, JS, Python — lock file detection
```

---

## Roadmap

- [ ] Linux toolchain support (gcc/clang path discovery, pkg-config)
- [ ] Parallel compilation (split TUs across threads)
- [ ] `uc add <package>` — install a dependency via the native package manager
- [ ] Watch mode (`uc watch`) — rebuild on file change
- [ ] Cross-compilation targets (`--target x86_64-pc-windows-msvc`)
- [ ] LSP-style error output for editor integration
- [ ] Plugin system for custom languages

---

## Contributing

PRs welcome. The easiest place to start is adding a new language resolver under
`src/resolver/` and wiring it into `resolver/mod.rs` and `planner.rs`.
