#![allow(clippy::unwrap_used)]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const DOCUMENTS: [&str; 3] = ["README.md", "CHANGELOG.md", "AGENTS.md"];
const DOCUMENT_FOLDERS: [&str; 3] = ["docs", "specs", "examples"];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn documents() -> Vec<PathBuf> {
    // A deleted folder must surface as the broken links pointing into it, not as a panic here.
    let folders = DOCUMENT_FOLDERS
        .iter()
        .flat_map(|folder| fs::read_dir(root().join(folder)).into_iter().flatten())
        .map(|entry| entry.unwrap().path());
    let mut documents: Vec<_> = DOCUMENTS
        .iter()
        .map(|name| root().join(name))
        .chain(folders.filter(|path| path.extension().is_some_and(|ext| ext == "md")))
        .collect();
    documents.sort();
    documents
}

fn outside_fences(markdown: &str) -> Vec<&str> {
    let mut fence: Option<&str> = None;
    let mut lines = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        let marker = match trimmed.chars().next() {
            Some(c @ ('`' | '~')) => &trimmed[..trimmed.len() - trimmed.trim_start_matches(c).len()],
            _ => "",
        };
        match fence {
            Some(open) if marker.starts_with(open) && trimmed[marker.len()..].trim().is_empty() => fence = None,
            Some(_) => {}
            None if marker.len() >= 3 => fence = Some(marker),
            None => lines.push(line),
        }
    }
    lines
}

fn without_code_spans(line: &str) -> String {
    let mut kept = String::new();
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        kept.push_str(&rest[..start]);
        let run = rest[start..].chars().take_while(|&c| c == '`').count();
        let after = &rest[start + run..];
        match after.find(&"`".repeat(run)) {
            Some(end) => rest = &after[end + run..],
            None => return kept + &rest[start..],
        }
    }
    kept + rest
}

fn targets(markdown: &str) -> Vec<String> {
    outside_fences(markdown)
        .into_iter()
        .map(without_code_spans)
        .flat_map(|line| inline_targets(&line).into_iter().chain(reference_target(&line)))
        .collect()
}

fn inline_targets(line: &str) -> Vec<String> {
    line.split("](").skip(1).filter_map(|after| after.split_once(')')).map(|(inside, _)| destination(inside)).collect()
}

fn reference_target(line: &str) -> Option<String> {
    let definition = line.trim_start().strip_prefix('[')?;
    let (label, target) = definition.split_once("]:")?;
    (!label.starts_with('^')).then(|| destination(target))
}

fn destination(text: &str) -> String {
    let text = text.trim();
    let target = match text.strip_prefix('<') {
        Some(bracketed) => bracketed.split('>').next().unwrap_or_default(),
        None => text.split_whitespace().next().unwrap_or_default(),
    };
    target.to_owned()
}

fn is_external(target: &str) -> bool {
    target.contains("://") || target.starts_with("mailto:")
}

/// GitHub's heading anchor: lowercase, punctuation dropped except `-` and `_`, spaces as `-`.
fn slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|&c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

fn anchors(markdown: &str) -> HashSet<String> {
    let mut repeats: HashMap<String, usize> = HashMap::new();
    let mut anchors = HashSet::new();
    for heading in outside_fences(markdown).into_iter().filter_map(heading) {
        let base = slug(heading);
        let seen = repeats.entry(base.clone()).or_default();
        anchors.insert(if *seen == 0 { base } else { format!("{base}-{seen}") });
        *seen += 1;
    }
    anchors
}

fn heading(line: &str) -> Option<&str> {
    let level = line.chars().take_while(|&c| c == '#').count();
    let text = line[level..].strip_prefix(' ').filter(|_| (1..=6).contains(&level))?;
    Some(text.trim_end_matches(['#', ' ']))
}

fn problem(document: &Path, target: &str) -> Option<&'static str> {
    let (path, fragment) = target.split_once('#').map_or((target, None), |(path, fragment)| (path, Some(fragment)));
    let file = if path.is_empty() { document.to_path_buf() } else { document.parent().unwrap().join(path) };
    if !file.exists() {
        return Some("does not exist");
    }
    let fragment = fragment.filter(|_| file.extension().is_some_and(|ext| ext == "md"))?;
    (!anchors(&fs::read_to_string(&file).unwrap()).contains(fragment)).then_some("names no heading")
}

#[test]
fn every_relative_link_in_the_documentation_resolves() {
    let broken: Vec<_> = documents()
        .iter()
        .flat_map(|document| {
            let name = document.strip_prefix(root()).unwrap().display();
            targets(&fs::read_to_string(document).unwrap())
                .into_iter()
                .filter(|target| !is_external(target))
                .filter_map(|target| problem(document, &target).map(|problem| format!("{name}: {target} {problem}")))
                .collect::<Vec<_>>()
        })
        .collect();

    assert!(broken.is_empty(), "broken links:\n{}", broken.join("\n"));
}

#[test]
fn targets_cover_links_images_and_references_but_not_footnotes() {
    let markdown = "See [a](a.md#top \"title\") and ![b](<pics/b c.png>).\n[ref]: ref.md\n[^note]: a footnote";

    assert_eq!(targets(markdown), ["a.md#top", "pics/b c.png", "ref.md"]);
}

#[test]
fn targets_skip_code_blocks_and_code_spans() {
    let markdown = "````md\n```\n[a](fenced.md)\n```\n````\nUse `[b](span.md)` or ``[c](`d`.md)`` then [e](kept.md)";

    assert_eq!(targets(markdown), ["kept.md"]);
}

#[test]
fn a_link_needs_an_existing_file_and_heading() {
    let folder = tempfile::tempdir().unwrap();
    let document = folder.path().join("doc.md");
    fs::write(&document, "# Top\n").unwrap();
    fs::write(folder.path().join("other.md"), "## Usage\n").unwrap();

    let problems = ["#top", "other.md#usage", "other.md", "#missing", "other.md#top", "gone.md"].map(|target| problem(&document, target));

    assert_eq!(problems, [None, None, None, Some("names no heading"), Some("names no heading"), Some("does not exist")]);
}

#[test]
fn slugs_follow_github_rules() {
    assert_eq!(slug("Shell completions and man page"), "shell-completions-and-man-page");
    assert_eq!(slug("`mrk update`: what it does?"), "mrk-update-what-it-does");
    assert_eq!(slug("snake_case and kebab-case"), "snake_case-and-kebab-case");
}

#[test]
fn repeated_headings_get_numbered_anchors() {
    let anchors = anchors("# Notes\n## Notes\n### Notes ##\n```\n# Not a heading\n```\n#hashtag\n## The `mrk` pager");

    assert_eq!(anchors, HashSet::from(["notes", "notes-1", "notes-2", "the-mrk-pager"].map(str::to_owned)));
}
