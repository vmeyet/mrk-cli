use std::io::{self, BufWriter, Write};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use super::UsageError;
use crate::config::MIN_WIDTH;
use crate::document::{Document, Settings};
use crate::terminal::pager::{self, Rendered, Session};
use crate::terminal::{self, Align, Capabilities};
use crate::theme::{self, Theme};

const SIDE_MARGINS: usize = 4;

/// Where the rendered document goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    Print,
    Pager,
    /// The `MRK_PAGER` command, split into its words.
    Command(Vec<String>),
}

/// The built-in pager, or the `MRK_PAGER` command when set, but only when asked for and stdout is a terminal.
/// The command is split like a shell would split words and run without one, so nothing in it expands.
pub fn choose(wants_pager: bool, is_terminal: bool, command: Option<&str>) -> Result<Output> {
    if !wants_pager || !is_terminal {
        return Ok(Output::Print);
    }
    let words = command
        .map(shell_words::split)
        .transpose()
        .with_context(|| UsageError("MRK_PAGER does not split into words".to_owned()))?
        .unwrap_or_default();
    Ok(if words.is_empty() { Output::Pager } else { Output::Command(words) })
}

fn default_width(columns: u16) -> usize {
    usize::from(columns).saturating_sub(SIDE_MARGINS).min(theme::DEFAULT_WIDTH)
}

pub fn width(requested: Option<usize>, columns: u16) -> usize {
    requested.unwrap_or_else(|| default_width(columns)).max(MIN_WIDTH)
}

/// Everything that decides the layout except the window width, so the pager can lay the document out again on resize.
pub struct Layout {
    pub source: String,
    pub requested_width: Option<usize>,
    pub align: Align,
    pub theme: Theme,
    pub capabilities: Capabilities,
}

impl Layout {
    pub fn render(&self, columns: u16) -> Rendered {
        let capabilities = Capabilities { columns, ..self.capabilities };
        let settings = Settings {
            width: width(self.requested_width, columns),
            theme: self.theme.clone(),
            cell: capabilities.graphics.map(|graphics| graphics.cell),
            hyperlinks: capabilities.hyperlinks,
        };
        let margin = terminal::margin(self.align, &capabilities, settings.width);
        Rendered { document: crate::markdown::render(&self.source, &settings), margin }
    }
}

fn write_all(out: &mut impl Write, document: &Document, capabilities: &Capabilities, margin: usize) -> io::Result<()> {
    terminal::write(document, capabilities, margin, out).and_then(|()| out.flush())
}

pub fn ignore_broken_pipe(written: io::Result<()>) -> io::Result<()> {
    match written {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
}

pub fn print(document: &Document, capabilities: &Capabilities, margin: usize) -> Result<()> {
    let written = write_all(&mut BufWriter::new(io::stdout().lock()), document, capabilities, margin);
    ignore_broken_pipe(written).context("writing to stdout")
}

fn pipe(words: &[String], layout: &Layout) -> Result<()> {
    let Some((program, arguments)) = words.split_first() else { bail!("MRK_PAGER is empty") };
    let Rendered { document, margin } = layout.render(layout.capabilities.columns);
    let mut child =
        Command::new(program).args(arguments).stdin(Stdio::piped()).spawn().with_context(|| format!("starting MRK_PAGER {program}"))?;
    let written = child.stdin.take().map_or(Ok(()), |mut stdin| write_all(&mut stdin, &document, &layout.capabilities, margin));
    let status = child.wait().with_context(|| format!("waiting for MRK_PAGER {program}"))?;
    ignore_broken_pipe(written).context("writing to MRK_PAGER")?;
    if !status.success() {
        bail!("MRK_PAGER {program} failed: {status}");
    }
    Ok(())
}

pub fn show(output: &Output, layout: &Layout, name: &str) -> Result<()> {
    match output {
        Output::Print => {
            let Rendered { document, margin } = layout.render(layout.capabilities.columns);
            print(&document, &layout.capabilities, margin)
        }
        Output::Pager => {
            let session = Session { name, palette: &layout.theme.palette, capabilities: &layout.capabilities };
            pager::run(&session, |columns| layout.render(columns)).context("running the pager")
        }
        Output::Command(words) => pipe(words, layout),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn the_default_width_leaves_margins_and_caps_at_100() {
        assert_eq!(width(None, 80), 76);
        assert_eq!(width(None, 200), 100);
        assert_eq!(width(None, 10), MIN_WIDTH);
        assert_eq!(width(Some(120), 80), 120);
    }

    fn words(list: &[&str]) -> Output {
        Output::Command(list.iter().map(|&word| word.to_owned()).collect())
    }

    #[test]
    fn the_pager_only_opens_when_asked_and_on_a_terminal() {
        assert_eq!(choose(false, true, None).unwrap(), Output::Print);
        assert_eq!(choose(true, false, Some("less -R")).unwrap(), Output::Print);
        assert_eq!(choose(true, true, None).unwrap(), Output::Pager);
        assert_eq!(choose(true, true, Some("  ")).unwrap(), Output::Pager);
    }

    #[test]
    fn mrk_pager_is_split_into_words_without_a_shell() {
        assert_eq!(choose(true, true, Some("less -R")).unwrap(), words(&["less", "-R"]));
        assert_eq!(choose(true, true, Some("'my pager' --title \"a b\" x\\ y")).unwrap(), words(&["my pager", "--title", "a b", "x y"]));
        assert_eq!(
            choose(true, true, Some("less $HOME; rm -rf ~ | cat")).unwrap(),
            words(&["less", "$HOME;", "rm", "-rf", "~", "|", "cat"])
        );
    }

    #[test]
    fn an_unbalanced_quote_is_refused() {
        assert!(choose(true, true, Some("less 'oops")).unwrap_err().is::<UsageError>());
    }
}
