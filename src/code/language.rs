use syntect::parsing::{SyntaxReference, SyntaxSet};

const PLAIN: [&str; 5] = ["text", "txt", "plain", "plaintext", "none"];

/// Fence names bat's syntaxes do not claim, and the token of the syntax they mean.
const ALIASES: [(&str, &str); 16] = [
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

/// The syntax a fence language names, by token, extension or alias; `None` for plain text and unknown names.
pub fn find<'a>(language: &str, syntaxes: &'a SyntaxSet) -> Option<&'a SyntaxReference> {
    let token = language.to_ascii_lowercase();
    if PLAIN.contains(&token.as_str()) {
        return None;
    }

    let token = ALIASES.iter().find(|(alias, _)| *alias == token).map_or(token.as_str(), |(_, target)| target);
    syntaxes.find_syntax_by_token(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syntax_name(language: &str) -> Option<String> {
        find(language, &two_face::syntax::extra_newlines()).map(|syntax| syntax.name.clone())
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
            ("hcl", "Terraform"),
        ];

        for (language, name) in expected {
            assert_eq!(syntax_name(language).as_deref(), Some(name), "{language}");
        }
    }

    #[test]
    fn plain_and_unknown_names_have_no_syntax() {
        for language in ["text", "TXT", "plaintext", "klingon"] {
            assert_eq!(syntax_name(language), None, "{language}");
        }
    }
}
