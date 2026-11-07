#[test]
fn detects_markup_and_config_languages() {
    let dir = temp_project("config-langs");
    let cases = [
        ("notes.md", "# Title\n", LanguageId::Markdown),
        ("data.json", "{\"a\": 1}\n", LanguageId::Json),
        ("config.yaml", "a: true\n", LanguageId::Yaml),
        (
            "configuration.nix",
            "{ pkgs, ... }: { environment.systemPackages = [ ]; }\n",
            LanguageId::Nix,
        ),
    ];
    for (name, content, expected) in cases {
        let file = dir.join(name);
        fs::write(&file, content).unwrap();
        let app = app_with_file(&file);
        let language = app.editor.active_document().unwrap().buffer.language;
        assert_eq!(language, expected, "detected {name} as {language:?}");
    }
    // `Cargo.toml` is a TOML file, even though it marks a Rust project.
    let manifest = dir.join("Cargo.toml");
    let app = app_with_file(&manifest);
    assert_eq!(
        app.editor.active_document().unwrap().buffer.language,
        LanguageId::Toml
    );
    fs::remove_dir_all(&dir).ok();
}
