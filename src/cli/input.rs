use std::io::{self, IsTerminal, Read};
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::UsageError;
use crate::terminal::sanitize;

const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const USAGE_HINT: &str = "no input: name a Markdown file (mrk README.md) or pipe one in (cat notes.md | mrk)";

fn read_capped(reader: impl Read, name: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(MAX_INPUT_BYTES as u64 + 1).read_to_end(&mut bytes).with_context(|| format!("reading {name}"))?;
    if bytes.len() > MAX_INPUT_BYTES {
        bail!("{name} is larger than 8 MiB, the most mrk reads");
    }
    Ok(bytes)
}

/// Invalid UTF-8 is replaced rather than refused: a stray Latin-1 byte should not hide an otherwise readable file,
/// and U+FFFD is inert on a terminal.
fn decode(bytes: Vec<u8>, name: &str) -> String {
    String::from_utf8(bytes).unwrap_or_else(|error| {
        eprintln!("⚠ {} is not valid UTF-8: invalid bytes show as �", sanitize::text(name));
        String::from_utf8_lossy(error.as_bytes()).into_owned()
    })
}

fn read_file(path: &Path) -> Result<String> {
    let name = path.display().to_string();
    let file = std::fs::File::open(path).with_context(|| format!("reading {name}"))?;
    Ok(decode(read_capped(file, &name)?, &name))
}

fn read_stdin() -> Result<String> {
    Ok(decode(read_capped(io::stdin().lock(), "stdin")?, "stdin"))
}

pub fn read_input(file: Option<&Path>) -> Result<String> {
    match file {
        None if io::stdin().is_terminal() => Err(UsageError(USAGE_HINT.to_owned()).into()),
        Some(path) if path != Path::new("-") => read_file(path),
        _ => read_stdin(),
    }
}

/// What the pager's status bar calls the input: the file name without its directory, or `stdin`.
pub fn name(file: Option<&Path>) -> String {
    let file_name = file.filter(|path| *path != Path::new("-")).and_then(Path::file_name);
    file_name.map_or_else(|| "stdin".to_owned(), |name| name.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn oversized_input_is_refused() {
        let error = read_capped(io::repeat(b'a').take(MAX_INPUT_BYTES as u64 + 1), "big.md").unwrap_err().to_string();

        assert!(error.contains("big.md") && error.contains("8 MiB"), "{error}");
        assert_eq!(read_capped(io::repeat(b'a').take(MAX_INPUT_BYTES as u64), "ok.md").unwrap().len(), MAX_INPUT_BYTES);
    }

    #[test]
    fn invalid_utf8_is_replaced() {
        assert_eq!(decode(b"caf\xe9".to_vec(), "x.md"), "caf\u{fffd}");
    }

    #[test]
    fn stdin_is_named_stdin() {
        assert_eq!(name(None), "stdin");
        assert_eq!(name(Some(Path::new("-"))), "stdin");
        assert_eq!(name(Some(Path::new("/home/me/docs/a.md"))), "a.md");
    }
}
