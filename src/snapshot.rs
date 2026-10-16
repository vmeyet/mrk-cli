//! What colour snapshot tests compare: above `document` and `terminal` both, so neither reaches into the other.

use crate::document::Document;
use crate::terminal::{Capabilities, ColorDepth};

/// The document as `terminal::write` prints it in truecolor with hyperlinks, `␛` for ESC.
pub(crate) fn ansi(document: &Document) -> String {
    let capabilities = Capabilities {
        color: ColorDepth::TrueColor,
        hyperlinks: true,
        graphics: None,
        double_height: false,
        background: None,
        columns: 80,
        rows: 24,
        is_terminal: true,
    };
    crate::terminal::ansi(document, &capabilities, 0).replace('\x1b', "␛")
}
