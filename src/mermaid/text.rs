use vt_mermaid::{GlyphSet, Role, Run};

use super::DiagramError;
use super::catch::caught;
use crate::document::{Line, Rgb, Span, Style};
use crate::theme::Palette;

/// The diagram as vt-mermaid draws it no wider than `width`: node borders in `muted`, edges and their tips in
/// `accent`, group frames in `subtle`, labels in `text`.
pub fn render(source: &str, width: usize, palette: &Palette) -> Result<Vec<Line>, DiagramError> {
    let max_width = u16::try_from(width).unwrap_or(u16::MAX);
    let canvas = caught(|| vt_mermaid::render(source, max_width))?.map_err(|error| DiagramError::invalid(&error))?;
    let lines: Vec<Line> = canvas.rows(GlyphSet::Unicode).map(|runs| line(runs, palette)).collect();
    if lines.is_empty() {
        return Err(DiagramError::Empty);
    }
    if lines.iter().any(|line| line.width() > width) {
        return Err(DiagramError::TooWide(width));
    }

    Ok(lines)
}

/// Every row is as wide as the canvas: the empty cells closing a shorter one are left out.
fn line(mut runs: Vec<Run<Role>>, palette: &Palette) -> Line {
    if let Some(last) = runs.last_mut().filter(|run| run.role == Role::Plain) {
        last.text.truncate(last.text.trim_end().len());
    }
    let drawn = runs.into_iter().filter(|run| !run.text.is_empty());
    Line::new(drawn.map(|run| Span::new(run.text, Style::fg(colour(run.role, palette)))).collect())
}

fn colour(role: Role, palette: &Palette) -> Rgb {
    match role {
        Role::Node => palette.muted,
        Role::Edge => palette.accent,
        Role::Group => palette.subtle,
        _ => palette.text,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::theme::MRK_DARK;

    const FLOWCHART: &str =
        "flowchart TD\n  subgraph ci [CI]\n    A[Build] --> B{Tests pass?}\n  end\n  B -->|yes| C[Deploy]\n  B -->|no| A";

    #[test]
    fn a_flowchart_draws_its_groups_nodes_edges_and_labels() {
        let lines = render(FLOWCHART, 80, &MRK_DARK.palette).unwrap();

        insta::assert_snapshot!(lines.iter().map(Line::plain).collect::<Vec<_>>().join("\n"));
    }

    #[cfg(feature = "cli")]
    #[test]
    fn each_role_takes_its_colour() {
        let lines = render(FLOWCHART, 80, &MRK_DARK.palette).unwrap();

        insta::assert_snapshot!(crate::snapshot::ansi(&crate::document::Document { blocks: vec![crate::document::Block::Lines(lines)] }));
    }

    #[test]
    fn rows_end_on_what_was_drawn() {
        let lines = render(FLOWCHART, 80, &MRK_DARK.palette).unwrap();

        assert!(lines.iter().all(|line| !line.plain().ends_with(' ')));
    }

    #[test]
    fn drawing_never_exceeds_the_width() {
        let source = "graph LR\n  A[Alpha] --> B[Bravo] --> C[Charlie] --> D[Delta] --> E[Echo] --> F[Foxtrot]";

        for width in [20, 40, 60, 80, 120] {
            match render(source, width, &MRK_DARK.palette) {
                Ok(lines) => assert!(lines.iter().all(|line| line.width() <= width), "width {width}"),
                Err(error) => assert!(error.to_string().starts_with("the drawing needs"), "{error:?}"),
            }
        }
    }

    #[test]
    fn sequence_state_class_and_er_diagrams_draw() {
        let sources = [
            ("sequenceDiagram\n  Alice->>Bob: Hello", "Alice"),
            ("stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy", "Busy"),
            ("classDiagram\n  Animal <|-- Duck", "Duck"),
            ("erDiagram\n  CUSTOMER ||--o{ ORDER : places", "ORDER"),
        ];

        for (source, label) in sources {
            let lines = render(source, 80, &MRK_DARK.palette).unwrap();

            assert!(lines.iter().any(|line| line.plain().contains(label)), "{source:?}");
        }
    }

    #[test]
    fn a_kind_vt_mermaid_does_not_draw_is_unsupported() {
        let error = render("pie\n  \"a\" : 1", 80, &MRK_DARK.palette).unwrap_err();

        assert_eq!(error.to_string(), "pie diagrams are not drawn as text yet");
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(matches!(render("not a diagram at all", 80, &MRK_DARK.palette), Err(DiagramError::Invalid(_))));
    }

    #[test]
    fn a_flowchart_with_nothing_after_its_header_has_nothing_to_draw() {
        assert_eq!(render("graph", 80, &MRK_DARK.palette), Err(DiagramError::Empty));
    }

    #[test]
    fn a_clicked_node_shows_its_label_alone() {
        let lines = render("flowchart LR\n a --> b\n click a \"https://x\"", 80, &MRK_DARK.palette).unwrap();

        assert!(lines.iter().all(|line| !line.plain().contains("https")));
    }
}
