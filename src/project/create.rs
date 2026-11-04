//! Deterministic, offline project scaffolding.
//!
//! Creating a project writes a small, conventional structure directly rather
//! than shelling out to `cargo`, `go` or `pip`. That keeps the operation
//! instant, deterministic and usable with no network and no toolchain, which
//! matches Kemori's zero-configuration promise: the project is ready to code in
//! before the user sees the editor.
//!
//! Initialization never removes anything. If a write fails after the directory
//! was created, the outcome carries the partially created root so the UI can
//! explain what happened without destroying the user's files.

use std::path::{Path, PathBuf};

use crate::language::id::LanguageId;

/// Languages Kemori can scaffold, in the order it presents them.
pub const CREATABLE: &[LanguageId] = &[
    LanguageId::Rust,
    LanguageId::Go,
    LanguageId::Python,
    LanguageId::TypeScript,
    LanguageId::JavaScript,
    LanguageId::Java,
    LanguageId::CSharp,
    LanguageId::Php,
    LanguageId::Lua,
    LanguageId::Kotlin,
    LanguageId::Ruby,
    LanguageId::Elixir,
    LanguageId::Dart,
    LanguageId::Swift,
    LanguageId::Perl,
    LanguageId::Sql,
    LanguageId::Assembly,
    LanguageId::Html,
    LanguageId::Shell,
    LanguageId::Nix,
    LanguageId::C,
    LanguageId::Cpp,
];

/// Whether Kemori can generate a conventional project for `language`.
pub fn is_creatable(language: LanguageId) -> bool {
    CREATABLE.contains(&language)
}

/// A short description of what a language's template generates.
pub fn describe(language: LanguageId) -> &'static str {
    match language {
        LanguageId::Rust => "Cargo.toml + src/main.rs",
        LanguageId::Go => "go.mod + main.go",
        LanguageId::Python => "pyproject.toml + src/<package>",
        LanguageId::TypeScript => "package.json + tsconfig.json",
        LanguageId::JavaScript => "package.json + src/index.js",
        LanguageId::Java => "pom.xml + src/main/java/<package>",
        LanguageId::CSharp => "a .csproj + Program.cs",
        LanguageId::Php => "composer.json + index.php",
        LanguageId::Lua => "init.lua + .luarc.json",
        LanguageId::Kotlin => "Gradle Kotlin DSL + src/main/kotlin",
        LanguageId::Ruby => "Gemfile + lib/<name>.rb",
        LanguageId::Elixir => "mix.exs + lib/<name>.ex",
        LanguageId::Dart => "pubspec.yaml + bin/main.dart",
        LanguageId::Swift => "Package.swift + Sources/<name>",
        LanguageId::Perl => "an executable <name>.pl",
        LanguageId::Sql => "schema.sql",
        LanguageId::Assembly => "main.asm + Makefile",
        LanguageId::Html => "index.html + style.css",
        LanguageId::Shell => "an executable <name>.sh",
        LanguageId::Nix => "flake.nix + .gitignore",
        LanguageId::C => "CMakeLists.txt + src/main.c",
        LanguageId::Cpp => "CMakeLists.txt + src/main.cpp",
        _ => "a minimal project structure",
    }
}

/// The outcome of creating a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreateOutcome {
    /// The project was created; `files` lists what was written, for reporting.
    Created { root: PathBuf, files: Vec<PathBuf> },
    /// Creation failed. `root` is `Some` when a partial directory was left
    /// behind (Kemori never deletes it).
    Failed {
        root: Option<PathBuf>,
        message: String,
    },
}

impl CreateOutcome {
    /// The project root, whether creation succeeded or was partial.
    pub fn root(&self) -> Option<&Path> {
        match self {
            CreateOutcome::Created { root, .. } => Some(root),
            CreateOutcome::Failed { root, .. } => root.as_deref(),
        }
    }
}

/// Validate a project directory name, independent of the target language.
///
/// Returns a human-readable reason when the name cannot be used safely on the
/// current platform.
pub fn validate_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Enter a project name".to_string());
    }
    if trimmed == "." || trimmed == ".." {
        return Err("That name is not allowed".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err("A name cannot contain path separators".to_string());
    }
    if trimmed.chars().any(|c| c == '\0' || c.is_control()) {
        return Err("A name cannot contain control characters".to_string());
    }
    // The name is interpolated into generated source, shell scripts and HTML;
    // reject characters that could break out of those contexts.
    const UNSAFE: &[char] = &['"', '\\', '`', '$', '<', '>', '&'];
    if let Some(bad) = trimmed.chars().find(|c| UNSAFE.contains(c)) {
        return Err(format!("A name cannot contain `{bad}`"));
    }
    if trimmed.chars().count() > 128 {
        return Err("That name is too long".to_string());
    }
    if cfg!(windows) {
        if trimmed.ends_with('.') || trimmed.ends_with(' ') {
            return Err("Windows does not allow names ending in a dot or space".to_string());
        }
        let stem = trimmed.split('.').next().unwrap_or("").to_ascii_uppercase();
        const RESERVED: &[&str] = &[
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];
        if RESERVED.contains(&stem.as_str()) {
            return Err("That name is reserved on Windows".to_string());
        }
    }
    Ok(())
}

/// Create a project named `name` inside `parent` for `language`.
pub fn create(parent: &Path, name: &str, language: LanguageId) -> CreateOutcome {
    let name = name.trim();
    if let Err(message) = validate_name(name) {
        return CreateOutcome::Failed {
            root: None,
            message,
        };
    }
    let root = parent.join(name);
    if root.exists() {
        return CreateOutcome::Failed {
            root: None,
            message: format!("{} already exists", root.display()),
        };
    }
    if let Err(err) = std::fs::create_dir_all(&root) {
        return CreateOutcome::Failed {
            root: None,
            message: format!("could not create the folder: {err}"),
        };
    }

    let result = match language {
        LanguageId::Rust => rust(&root, name),
        LanguageId::Go => go(&root, name),
        LanguageId::Python => python(&root, name),
        LanguageId::TypeScript => typescript(&root, name),
        LanguageId::JavaScript => javascript(&root, name),
        LanguageId::Java => java(&root, name),
        LanguageId::CSharp => csharp(&root, name),
        LanguageId::Php => php(&root, name),
        LanguageId::Lua => lua(&root, name),
        LanguageId::Kotlin => kotlin(&root, name),
        LanguageId::Ruby => ruby(&root, name),
        LanguageId::Elixir => elixir(&root, name),
        LanguageId::Dart => dart(&root, name),
        LanguageId::Swift => swift(&root, name),
        LanguageId::Perl => perl(&root, name),
        LanguageId::Sql => sql(&root, name),
        LanguageId::Assembly => assembly(&root, name),
        LanguageId::Html => html(&root, name),
        LanguageId::Shell => shell(&root, name),
        LanguageId::Nix => nix(&root, name),
        LanguageId::C => c(&root, name),
        LanguageId::Cpp => cpp(&root, name),
        _ => Ok(Vec::new()),
    };

    match result {
        Ok(files) => CreateOutcome::Created { root, files },
        Err(message) => CreateOutcome::Failed {
            root: Some(root),
            message,
        },
    }
}

/// Write `contents` to `path`, returning the path on success.
fn write(root: &Path, relative: &str, contents: &str) -> Result<PathBuf, String> {
    let path = root.join(relative);
    crate::filesystem::write_string(&path, contents)
        .map_err(|err| format!("could not write {}: {err}", path.display()))?;
    Ok(path)
}

fn rust(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let package = crate_name(name);
    let manifest = format!(
        "[package]\nname = \"{package}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n"
    );
    let main = format!("fn main() {{\n    println!(\"Hello from {name}!\");\n}}\n");
    Ok(vec![
        write(root, "Cargo.toml", &manifest)?,
        write(root, "src/main.rs", &main)?,
        write(root, ".gitignore", "/target\n")?,
    ])
}

fn go(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let module = module_name(name);
    let manifest = format!("module {module}\n\ngo 1.18\n");
    let main = format!(
        "package main\n\nimport \"fmt\"\n\nfunc main() {{\n\tfmt.Println(\"Hello from {name}!\")\n}}\n"
    );
    Ok(vec![
        write(root, "go.mod", &manifest)?,
        write(root, "main.go", &main)?,
    ])
}

fn python(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let module = python_module(name);
    let manifest = format!(
        "[project]\nname = \"{name}\"\nversion = \"0.1.0\"\ndescription = \"A new project created with Kemori\"\nrequires-python = \">=3.9\"\n\n[build-system]\nrequires = [\"setuptools>=61\"]\nbuild-backend = \"setuptools.build_meta\"\n"
    );
    let init = format!("\"\"\"{name} package.\"\"\"\n\n__version__ = \"0.1.0\"\n");
    let entry = "def main() -> None:\n    print(\"Hello from your new project!\")\n\n\nif __name__ == \"__main__\":\n    main()\n";
    let readme = format!("# {name}\n\nA Python project created with Kemori.\n");
    Ok(vec![
        write(root, "pyproject.toml", &manifest)?,
        write(root, &format!("src/{module}/__init__.py"), &init)?,
        write(root, &format!("src/{module}/__main__.py"), entry)?,
        write(root, "README.md", &readme)?,
    ])
}

fn shell(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let script = format!(
        "#!/usr/bin/env bash\nset -euo pipefail\n\nmain() {{\n    echo \"Hello from {name}!\"\n}}\n\nmain \"$@\"\n"
    );
    let readme = format!("# {name}\n\nA shell project created with Kemori.\n");
    let script_path = write(root, &format!("{name}.sh"), &script)?;
    make_executable(&script_path);
    Ok(vec![script_path, write(root, "README.md", &readme)?])
}

fn nix(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let flake = format!(
        "{{\n  description = \"A Nix flake created with Kemori ({name})\";\n\n  inputs.nixpkgs.url = \"github:NixOS/nixpkgs/nixos-unstable\";\n\n  outputs =\n    {{ self, nixpkgs }}:\n    let\n      systems = [\n        \"x86_64-linux\"\n        \"aarch64-linux\"\n        \"x86_64-darwin\"\n        \"aarch64-darwin\"\n      ];\n      forAllSystems = nixpkgs.lib.genAttrs systems;\n    in\n    {{\n      packages = forAllSystems (system: {{\n        default = nixpkgs.legacyPackages.${{system}}.hello;\n      }});\n    }};\n}}\n"
    );
    Ok(vec![
        write(root, "flake.nix", &flake)?,
        write(root, ".gitignore", "result\nresult-*\n")?,
    ])
}

fn typescript(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let package = module_name(name);
    let manifest = format!(
        "{{\n  \"name\": \"{package}\",\n  \"version\": \"0.1.0\",\n  \"private\": true,\n  \"type\": \"module\",\n  \"scripts\": {{\n    \"build\": \"tsc\",\n    \"start\": \"node dist/index.js\"\n  }},\n  \"devDependencies\": {{\n    \"typescript\": \"^5.0.0\"\n  }}\n}}\n"
    );
    let tsconfig = "{\n  \"compilerOptions\": {\n    \"target\": \"ES2022\",\n    \"module\": \"NodeNext\",\n    \"moduleResolution\": \"NodeNext\",\n    \"outDir\": \"dist\",\n    \"rootDir\": \"src\",\n    \"strict\": true,\n    \"esModuleInterop\": true,\n    \"skipLibCheck\": true\n  },\n  \"include\": [\"src\"]\n}\n";
    let index = format!(
        "function main(): void {{\n  console.log(\"Hello from {name}!\");\n}}\n\nmain();\n"
    );
    Ok(vec![
        write(root, "package.json", &manifest)?,
        write(root, "tsconfig.json", tsconfig)?,
        write(root, "src/index.ts", &index)?,
        write(root, ".gitignore", "node_modules\ndist\n")?,
    ])
}

fn javascript(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let package = module_name(name);
    let manifest = format!(
        "{{\n  \"name\": \"{package}\",\n  \"version\": \"0.1.0\",\n  \"private\": true,\n  \"type\": \"module\",\n  \"scripts\": {{\n    \"start\": \"node src/index.js\"\n  }}\n}}\n"
    );
    let index =
        format!("function main() {{\n  console.log(\"Hello from {name}!\");\n}}\n\nmain();\n");
    Ok(vec![
        write(root, "package.json", &manifest)?,
        write(root, "src/index.js", &index)?,
        write(root, ".gitignore", "node_modules\n")?,
    ])
}

fn c(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let project = crate_name(name);
    let cmake = format!(
        "cmake_minimum_required(VERSION 3.16)\nproject({project} C)\n\nset(CMAKE_C_STANDARD 17)\nset(CMAKE_C_STANDARD_REQUIRED ON)\n\nadd_executable({project} src/main.c)\n"
    );
    let main = "#include <stdio.h>\n\nint main(void) {\n    printf(\"Hello from your new project!\\n\");\n    return 0;\n}\n";
    Ok(vec![
        write(root, "CMakeLists.txt", &cmake)?,
        write(root, "src/main.c", main)?,
        write(root, ".gitignore", "build/\n")?,
    ])
}

fn cpp(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let project = crate_name(name);
    let cmake = format!(
        "cmake_minimum_required(VERSION 3.16)\nproject({project} CXX)\n\nset(CMAKE_CXX_STANDARD 20)\nset(CMAKE_CXX_STANDARD_REQUIRED ON)\n\nadd_executable({project} src/main.cpp)\n"
    );
    let main = "#include <iostream>\n\nint main() {\n    std::cout << \"Hello from your new project!\\n\";\n    return 0;\n}\n";
    Ok(vec![
        write(root, "CMakeLists.txt", &cmake)?,
        write(root, "src/main.cpp", main)?,
        write(root, ".gitignore", "build/\n")?,
    ])
}

fn java(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let artifact = module_name(name);
    let package = python_module(name);
    let pom = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <project xmlns=\"http://maven.apache.org/POM/4.0.0\"\n\
         \x20        xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n\
         \x20        xsi:schemaLocation=\"http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd\">\n\
         \x20 <modelVersion>4.0.0</modelVersion>\n\
         \x20 <groupId>com.example</groupId>\n\
         \x20 <artifactId>{artifact}</artifactId>\n\
         \x20 <version>0.1.0</version>\n\
         \x20 <properties>\n\
         \x20   <maven.compiler.release>21</maven.compiler.release>\n\
         \x20   <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>\n\
         \x20 </properties>\n\
         </project>\n"
    );
    let app = format!(
        "package com.example.{package};\n\npublic final class App {{\n    public static void main(String[] args) {{\n        System.out.println(\"Hello from {name}!\");\n    }}\n}}\n"
    );
    let path = format!("src/main/java/com/example/{package}/App.java");
    Ok(vec![
        write(root, "pom.xml", &pom)?,
        write(root, &path, &app)?,
        write(root, ".gitignore", "target/\n*.class\n")?,
    ])
}

fn csharp(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let project = crate_name(name);
    let csproj = "<Project Sdk=\"Microsoft.NET.Sdk\">\n\
         \x20 <PropertyGroup>\n\
         \x20   <OutputType>Exe</OutputType>\n\
         \x20   <TargetFramework>net10.0</TargetFramework>\n\
         \x20   <ImplicitUsings>enable</ImplicitUsings>\n\
         \x20   <Nullable>enable</Nullable>\n\
         \x20 </PropertyGroup>\n\
         </Project>\n";
    let program = format!("Console.WriteLine(\"Hello from {name}!\");\n");
    Ok(vec![
        write(root, &format!("{project}.csproj"), csproj)?,
        write(root, "Program.cs", &program)?,
        write(root, ".gitignore", "bin/\nobj/\n")?,
    ])
}

fn php(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let package = crate_name(name);
    let composer = format!(
        "{{\n  \"name\": \"kemori/{package}\",\n  \"description\": \"A new project created with Kemori\",\n  \"type\": \"project\",\n  \"require\": {{}}\n}}\n"
    );
    let index = format!(
        "<?php\n\ndeclare(strict_types=1);\n\nfunction main(): void\n{{\n    echo \"Hello from {name}!\\n\";\n}}\n\nmain();\n"
    );
    Ok(vec![
        write(root, "composer.json", &composer)?,
        write(root, "index.php", &index)?,
        write(root, ".gitignore", "/vendor/\n")?,
    ])
}

fn lua(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let script = format!(
        "local M = {{}}\n\nfunction M.greet()\n  print(\"Hello from {name}!\")\nend\n\nreturn M\n"
    );
    let config = "{\n  \"runtime.version\": \"Lua 5.4\"\n}\n";
    Ok(vec![
        write(root, "init.lua", &script)?,
        write(root, ".luarc.json", config)?,
        write(root, ".gitignore", "*.luac\n")?,
    ])
}

fn kotlin(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let settings = format!("rootProject.name = \"{name}\"\n");
    let build = "plugins {\n    kotlin(\"jvm\") version \"2.1.0\"\n    application\n}\n\nrepositories {\n    mavenCentral()\n}\n\napplication {\n    mainClass.set(\"MainKt\")\n}\n\nkotlin {\n    jvmToolchain(21)\n}\n";
    let main = format!("fun main() {{\n    println(\"Hello from {name}!\")\n}}\n");
    Ok(vec![
        write(root, "settings.gradle.kts", &settings)?,
        write(root, "build.gradle.kts", build)?,
        write(root, "src/main/kotlin/Main.kt", &main)?,
        write(root, ".gitignore", ".gradle/\nbuild/\n")?,
    ])
}

fn ruby(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let module = python_module(name);
    let gemfile = "source \"https://rubygems.org\"\n\ngem \"rake\"\n";
    let lib = format!(
        "# frozen_string_literal: true\n\nmodule {pascal}\n  def self.greet\n    puts \"Hello from {name}!\"\n  end\nend\n\n{pascal}.greet if $PROGRAM_NAME == __FILE__\n",
        pascal = pascal_case(name)
    );
    Ok(vec![
        write(root, "Gemfile", gemfile)?,
        write(root, &format!("lib/{module}.rb"), &lib)?,
        write(root, ".gitignore", "*.gem\n.bundle/\nvendor/bundle/\n")?,
    ])
}

fn elixir(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let app = python_module(name);
    let mix = format!(
        "defmodule {pascal}.MixProject do\n  use Mix.Project\n\n  def project do\n    [\n      app: :{app},\n      version: \"0.1.0\",\n      elixir: \"~> 1.15\"\n    ]\n  end\nend\n",
        pascal = pascal_case(name)
    );
    let lib = format!(
        "defmodule {pascal} do\n  @moduledoc \"A new project created with Kemori.\"\n\n  def hello do\n    IO.puts(\"Hello from {name}!\")\n  end\nend\n",
        pascal = pascal_case(name)
    );
    Ok(vec![
        write(root, "mix.exs", &mix)?,
        write(root, &format!("lib/{app}.ex"), &lib)?,
        write(root, ".gitignore", "_build/\ndeps/\n*.ez\n")?,
    ])
}

fn dart(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let package = python_module(name);
    let pubspec = format!(
        "name: {package}\ndescription: A new project created with Kemori.\nversion: 0.1.0\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n"
    );
    let main = format!("void main() {{\n  print('Hello from {name}!');\n}}\n");
    Ok(vec![
        write(root, "pubspec.yaml", &pubspec)?,
        write(root, "bin/main.dart", &main)?,
        write(root, ".gitignore", ".dart_tool/\nbuild/\n")?,
    ])
}

fn swift(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let module = python_module(name);
    let manifest = format!(
        "// swift-tools-version:5.9\nimport PackageDescription\n\nlet package = Package(\n    name: \"{name}\",\n    targets: [\n        .executableTarget(name: \"{module}\")\n    ]\n)\n"
    );
    let main = format!("print(\"Hello from {name}!\")\n");
    let readme = format!("# {name}\n\nA Swift package created with Kemori.\n");
    Ok(vec![
        write(root, "Package.swift", &manifest)?,
        write(root, &format!("Sources/{module}/main.swift"), &main)?,
        write(root, "README.md", &readme)?,
        write(root, ".gitignore", ".build/\n.swiftpm/\n")?,
    ])
}

fn perl(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let script = format!(
        "#!/usr/bin/env perl\nuse strict;\nuse warnings;\n\nsub main {{\n    print \"Hello from {name}!\\n\";\n}}\n\nmain();\n"
    );
    let readme = format!("# {name}\n\nA Perl project created with Kemori.\n");
    let script_path = write(root, &format!("{name}.pl"), &script)?;
    make_executable(&script_path);
    Ok(vec![script_path, write(root, "README.md", &readme)?])
}

fn sql(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let schema = format!(
        "-- {name}: a starter schema.\n\nCREATE TABLE example (\n    id INTEGER PRIMARY KEY,\n    name TEXT NOT NULL,\n    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP\n);\n\nINSERT INTO example (name) VALUES ('hello');\n"
    );
    let readme = format!("# {name}\n\nA SQL project created with Kemori.\n");
    Ok(vec![
        write(root, "schema.sql", &schema)?,
        write(root, "README.md", &readme)?,
    ])
}

fn assembly(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    // A conservative x86-64 NASM skeleton; assembly is target-specific, so the
    // generated file states its assumption explicitly.
    let source = format!(
        "; {name}: x86-64 NASM skeleton.\n; Build with: make\n\n    global _start\n\n    section .text\n_start:\n    mov rax, 60     ; sys_exit\n    xor rdi, rdi    ; status 0\n    syscall\n"
    );
    let makefile = "main: main.asm\n\tnasm -f elf64 main.asm -o main.o\n\tld main.o -o main\n\nclean:\n\trm -f main main.o\n";
    Ok(vec![
        write(root, "main.asm", &source)?,
        write(root, "Makefile", makefile)?,
        write(root, ".gitignore", "*.o\nmain\n")?,
    ])
}

fn html(root: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let page = format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" />\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n    <title>{name}</title>\n    <link rel=\"stylesheet\" href=\"style.css\" />\n  </head>\n  <body>\n    <main class=\"card\">\n      <h1>Hello from {name}!</h1>\n      <p>Edit <code>index.html</code> to get started.</p>\n    </main>\n  </body>\n</html>\n"
    );
    let stylesheet = ":root {\n  color-scheme: light dark;\n}\n\nbody {\n  margin: 0;\n  font-family: system-ui, sans-serif;\n  display: grid;\n  place-items: center;\n  min-height: 100vh;\n}\n\n.card {\n  padding: 2rem;\n  border-radius: 0.75rem;\n  border: 1px solid #8884;\n}\n";
    Ok(vec![
        write(root, "index.html", &page)?,
        write(root, "style.css", stylesheet)?,
    ])
}

/// Best-effort `chmod +x` for shell entry points on Unix.
fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            let _ = std::fs::set_permissions(path, permissions);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// A `CamelCase` identifier derived from the directory name (Elixir/Ruby
/// module names).
pub fn pascal_case(name: &str) -> String {
    let snake = python_module(name);
    let pascal: String = snake
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    if pascal.is_empty() {
        "App".to_string()
    } else {
        pascal
    }
}

/// A valid Cargo package name derived from the directory name.
pub fn crate_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    out = out.trim_matches('-').to_string();
    if out.is_empty() {
        out = "app".to_string();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out = format!("app-{out}");
    }
    out
}

/// A valid Go module path derived from the directory name.
pub fn module_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    out = out.trim_matches('-').to_string();
    if out.is_empty() {
        out = "app".to_string();
    }
    out
}

/// A valid Python package name derived from the directory name.
pub fn python_module(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    out = out.trim_matches('_').to_string();
    if out.is_empty() {
        out = "app".to_string();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out = format!("_{out}");
    }
    out
}

#[cfg(test)]
#[path = "create_tests.rs"]
mod tests;
