use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kemori-create-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn rejects_invalid_names() {
    assert!(validate_name("").is_err());
    assert!(validate_name("   ").is_err());
    assert!(validate_name(".").is_err());
    assert!(validate_name("..").is_err());
    assert!(validate_name("a/b").is_err());
    assert!(validate_name("a\\b").is_err());
    assert!(validate_name("nul").is_ok()); // only reserved on Windows
    assert!(validate_name("my project").is_ok());
    assert!(validate_name("プロジェクト").is_ok());
}

#[test]
fn rejects_names_unsafe_for_generated_files() {
    // These characters would break out of the string literals or shell
    // snippets the templates interpolate the name into.
    for bad in ["a\"b", "a$b", "a`b", "<tag>", "a&b", "a\\b"] {
        assert!(validate_name(bad).is_err(), "`{bad}` should be rejected");
    }
    // Ordinary punctuation and unicode remain allowed.
    assert!(validate_name("my.project").is_ok());
    assert!(validate_name("café-münchen").is_ok());
}

#[test]
fn refuses_an_existing_target() {
    let dir = scratch("exists");
    let parent = dir.join("parent");
    std::fs::create_dir_all(parent.join("taken")).unwrap();
    let outcome = create(&parent, "taken", LanguageId::Rust);
    assert!(matches!(outcome, CreateOutcome::Failed { root: None, .. }));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_rust() {
    let dir = scratch("rust");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    let outcome = create(&parent, "my app", LanguageId::Rust);
    let CreateOutcome::Created { root, files } = outcome else {
        panic!("expected success, got {outcome:?}");
    };
    assert!(files.iter().any(|f| f.ends_with("Cargo.toml")));
    assert!(root.join("src/main.rs").is_file());
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("name = \"my-app\""));
    assert!(manifest.contains("edition = \"2024\""));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_go() {
    let dir = scratch("go");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    let outcome = create(&parent, "hello", LanguageId::Go);
    let CreateOutcome::Created { root, .. } = outcome else {
        panic!("expected success");
    };
    let manifest = std::fs::read_to_string(root.join("go.mod")).unwrap();
    assert_eq!(manifest, "module hello\n\ngo 1.18\n");
    assert!(root.join("main.go").is_file());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_python_with_an_importable_module() {
    let dir = scratch("python");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    let outcome = create(&parent, "My Service", LanguageId::Python);
    let CreateOutcome::Created { root, .. } = outcome else {
        panic!("expected success");
    };
    assert!(root.join("pyproject.toml").is_file());
    assert!(root.join("src/my_service/__init__.py").is_file());
    assert!(root.join("src/my_service/__main__.py").is_file());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_shell_and_marks_it_executable() {
    let dir = scratch("shell");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    let outcome = create(&parent, "deploy", LanguageId::Shell);
    let CreateOutcome::Created { root, .. } = outcome else {
        panic!("expected success");
    };
    let script = root.join("deploy.sh");
    assert!(script.is_file());
    let contents = std::fs::read_to_string(&script).unwrap();
    assert!(contents.starts_with("#!/usr/bin/env bash"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&script).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "script should be executable");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_a_nix_flake() {
    let dir = scratch("nix");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    let outcome = create(&parent, "My System", LanguageId::Nix);
    let CreateOutcome::Created { root, files } = outcome else {
        panic!("expected success, got {outcome:?}");
    };
    assert!(files.iter().any(|file| file.ends_with("flake.nix")));
    let flake = std::fs::read_to_string(root.join("flake.nix")).unwrap();
    assert!(flake.contains("description ="));
    assert!(flake.contains("github:NixOS/nixpkgs/nixos-unstable"));
    assert!(flake.contains("forAllSystems"));
    assert!(root.join(".gitignore").is_file());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_typescript_and_javascript() {
    let dir = scratch("web-projects");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();

    let CreateOutcome::Created { root, .. } = create(&parent, "My Web App", LanguageId::TypeScript)
    else {
        panic!("expected TypeScript success");
    };
    assert!(root.join("package.json").is_file());
    assert!(root.join("tsconfig.json").is_file());
    assert!(root.join("src/index.ts").is_file());
    let manifest = std::fs::read_to_string(root.join("package.json")).unwrap();
    assert!(manifest.contains("\"name\": \"my-web-app\""));

    let CreateOutcome::Created { root, .. } = create(&parent, "my-js-app", LanguageId::JavaScript)
    else {
        panic!("expected JavaScript success");
    };
    assert!(root.join("src/index.js").is_file());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_c_and_cpp() {
    let dir = scratch("c-projects");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();

    let CreateOutcome::Created { root, .. } = create(&parent, "so cool", LanguageId::C) else {
        panic!("expected C success");
    };
    assert!(root.join("CMakeLists.txt").is_file());
    assert!(root.join("src/main.c").is_file());
    let cmake = std::fs::read_to_string(root.join("CMakeLists.txt")).unwrap();
    assert!(cmake.contains("project(so-cool C)"));

    let CreateOutcome::Created { root, .. } = create(&parent, "app", LanguageId::Cpp) else {
        panic!("expected C++ success");
    };
    assert!(root.join("src/main.cpp").is_file());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_java_and_csharp() {
    let dir = scratch("jvm-dotnet");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();

    let CreateOutcome::Created { root, .. } = create(&parent, "My Service", LanguageId::Java)
    else {
        panic!("expected Java success");
    };
    assert!(root.join("pom.xml").is_file());
    assert!(
        root.join("src/main/java/com/example/my_service/App.java")
            .is_file()
    );
    let pom = std::fs::read_to_string(root.join("pom.xml")).unwrap();
    assert!(pom.contains("<artifactId>my-service</artifactId>"));
    assert!(pom.contains("maven.compiler.release>21"));

    let CreateOutcome::Created { root, .. } = create(&parent, "My App", LanguageId::CSharp) else {
        panic!("expected C# success");
    };
    assert!(root.join("my-app.csproj").is_file());
    assert!(root.join("Program.cs").is_file());
    let csproj = std::fs::read_to_string(root.join("my-app.csproj")).unwrap();
    assert!(csproj.contains("<TargetFramework>net10.0</TargetFramework>"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_html() {
    let dir = scratch("html");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    let CreateOutcome::Created { root, .. } = create(&parent, "My Site", LanguageId::Html) else {
        panic!("expected HTML success");
    };
    assert!(root.join("index.html").is_file());
    assert!(root.join("style.css").is_file());
    let page = std::fs::read_to_string(root.join("index.html")).unwrap();
    assert!(page.contains("<link rel=\"stylesheet\" href=\"style.css\""));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_php() {
    let dir = scratch("php");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    let CreateOutcome::Created { root, .. } = create(&parent, "My Service", LanguageId::Php) else {
        panic!("expected PHP success");
    };
    assert!(root.join("composer.json").is_file());
    assert!(root.join("index.php").is_file());
    let index = std::fs::read_to_string(root.join("index.php")).unwrap();
    assert!(index.starts_with("<?php"));
    assert!(index.contains("Hello from My Service!"));
    let composer = std::fs::read_to_string(root.join("composer.json")).unwrap();
    assert!(composer.contains("\"name\": \"kemori/my-service\""));
    // The generated project is recognised as a PHP project by its marker.
    assert_eq!(crate::project::ProjectKind::Php.language(), LanguageId::Php);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_lua() {
    let dir = scratch("lua");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    let CreateOutcome::Created { root, .. } = create(&parent, "my mod", LanguageId::Lua) else {
        panic!("expected Lua success");
    };
    assert!(root.join("init.lua").is_file());
    assert!(root.join(".luarc.json").is_file());
    let script = std::fs::read_to_string(root.join("init.lua")).unwrap();
    assert!(script.contains("Hello from my mod!"));
    assert_eq!(crate::project::ProjectKind::Lua.language(), LanguageId::Lua);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_kotlin() {
    let dir = scratch("kotlin");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    let CreateOutcome::Created { root, .. } = create(&parent, "My App", LanguageId::Kotlin) else {
        panic!("expected Kotlin success");
    };
    assert!(root.join("settings.gradle.kts").is_file());
    assert!(root.join("build.gradle.kts").is_file());
    assert!(root.join("src/main/kotlin/Main.kt").is_file());
    let main = std::fs::read_to_string(root.join("src/main/kotlin/Main.kt")).unwrap();
    assert!(main.contains("Hello from My App!"));
    assert_eq!(
        crate::project::ProjectKind::Kotlin.language(),
        LanguageId::Kotlin
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scaffolds_the_new_languages() {
    let dir = scratch("newlangs");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    let cases = [
        (LanguageId::Ruby, "Gemfile"),
        (LanguageId::Elixir, "mix.exs"),
        (LanguageId::Dart, "pubspec.yaml"),
        (LanguageId::Swift, "Package.swift"),
        (LanguageId::Perl, "sample-perl.pl"),
        (LanguageId::Sql, "schema.sql"),
        (LanguageId::Assembly, "main.asm"),
    ];
    for (language, file) in cases {
        let name = format!("sample-{}", language.slug());
        let CreateOutcome::Created { root, .. } = create(&parent, &name, language) else {
            panic!("expected {language:?} to scaffold");
        };
        assert!(
            root.join(file).is_file(),
            "{language:?} should create {file}"
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn handles_names_with_spaces_and_unicode() {
    let dir = scratch("names");
    let parent = dir.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    let outcome = create(&parent, "проект", LanguageId::Rust);
    let CreateOutcome::Created { root, .. } = outcome else {
        panic!("expected success");
    };
    assert!(root.join("Cargo.toml").is_file());
    assert_eq!(crate_name("проект"), "app");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn helpers_produce_safe_identifiers() {
    assert_eq!(crate_name("My App!"), "my-app");
    assert_eq!(crate_name("2cool"), "app-2cool");
    assert_eq!(module_name("My Go App"), "my-go-app");
    assert_eq!(python_module("My-Service 2"), "my_service_2");
    assert_eq!(python_module("2fast"), "_2fast");
}
