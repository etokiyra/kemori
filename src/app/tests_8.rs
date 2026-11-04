#[test]
fn detects_java_and_csharp() {
    let dir = temp_project("jvm-langs");
    let cases = [
        (
            "Main.java",
            "public class Main {\n    public static void main(String[] args) {}\n}\n",
            LanguageId::Java,
        ),
        (
            "Program.cs",
            "using System;\n\nclass Program {\n    static void Main() {}\n}\n",
            LanguageId::CSharp,
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
