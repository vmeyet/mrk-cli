use std::sync::LazyLock;

use syntect::parsing::{SyntaxReference, SyntaxSet};

pub(super) static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);

const PLAIN: &[&str] = &["text", "txt", "plain", "plaintext", "none"];

/// Fence names bat's syntaxes do not claim, and the token of the syntax they mean.
const ALIASES: &[(&str, &str)] = &[
    ("jsx", "js"),
    ("mjs", "js"),
    ("cjs", "js"),
    ("shell", "sh"),
    ("console", "sh"),
    ("shellsession", "sh"),
    ("terminal", "sh"),
    ("docker", "dockerfile"),
    ("containerfile", "dockerfile"),
    ("jsonc", "json"),
    ("json5", "json"),
    ("golang", "go"),
    ("csharp", "cs"),
    ("objc", "objective-c"),
    ("cuda", "cpp"),
    ("hcl", "terraform"),
];

/// The syntax a fence language names, by token, extension or alias, in any case; `None` for plain text, which never
/// loads `SYNTAXES`, and unknown names.
pub fn find(language: &str) -> Option<&'static SyntaxReference> {
    if PLAIN.iter().any(|plain| plain.eq_ignore_ascii_case(language)) {
        return None;
    }
    let token = ALIASES.iter().find(|(alias, _)| alias.eq_ignore_ascii_case(language)).map_or(language, |(_, target)| target);
    SYNTAXES.find_syntax_by_token(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syntax_name(language: &str) -> Option<&'static str> {
        find(language).map(|syntax| syntax.name.as_str())
    }

    #[test]
    fn common_fence_names_resolve() {
        let expected = [
            ("ts", "TypeScript"),
            ("tsx", "TypeScriptReact"),
            ("js", "JavaScript (Babel)"),
            ("jsx", "JavaScript (Babel)"),
            ("sh", "Bourne Again Shell (bash)"),
            ("zsh", "Bourne Again Shell (bash)"),
            ("console", "Bourne Again Shell (bash)"),
            ("yml", "YAML"),
            ("rs", "Rust"),
            ("Rust", "Rust"),
            ("py", "Python"),
            ("dockerfile", "Dockerfile"),
            ("Dockerfile", "Dockerfile"),
            ("diff", "Diff"),
            ("json", "JSON"),
            ("jsonc", "JSON"),
            ("toml", "TOML"),
            ("sql", "SQL"),
            ("golang", "Go"),
            ("GoLang", "Go"),
            ("hcl", "Terraform"),
        ];

        for (language, name) in expected {
            assert_eq!(syntax_name(language), Some(name), "{language}");
        }
    }

    #[test]
    fn plain_and_unknown_names_have_no_syntax() {
        for language in ["text", "TXT", "plaintext", "klingon"] {
            assert_eq!(syntax_name(language), None, "{language}");
        }
    }
}
