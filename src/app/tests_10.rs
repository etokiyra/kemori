#[test]
fn detects_c_and_cpp() {
    let dir = temp_project("c-langs");
    let cases = [
        (
            "main.c",
            "#include <stdio.h>\nint main(void) { return 0; }\n",
            LanguageId::C,
        ),
        (
            "app.cpp",
            "#include <iostream>\nint main() { std::cout << 1; }\n",
            LanguageId::Cpp,
        ),
        (
            "view.hpp",
            "class View { public: int w; };\n",
            LanguageId::Cpp,
        ),
    ];
    for (name, content, expected) in cases {
        let file = dir.join(name);
        fs::write(&file, content).unwrap();
        let app = app_with_file(&file);
        let language = app.editor.active_document().unwrap().buffer.language;
        assert_eq!(language, expected, "detected {name} as {language:?}");
    }
    fs::remove_dir_all(&dir).ok();
}
