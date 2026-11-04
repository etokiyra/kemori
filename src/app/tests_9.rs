#[test]
fn detects_html_and_css() {
    let dir = temp_project("web-markup");
    let cases = [
        (
            "index.html",
            "<!DOCTYPE html>\n<html><body><h1>Hi</h1></body></html>\n",
            LanguageId::Html,
        ),
        (
            "style.css",
            "body {\n  color: #333;\n  display: flex;\n}\n",
            LanguageId::Css,
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
