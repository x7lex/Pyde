//! The card shown over the editor when the pointer rests on a token: its
//! diagnostics, signature and documentation.

use leptos::prelude::*;
use pyde_analysis::highlight;
use pyde_analysis::protocol::{DiagnosticSeverity, HoverInfo};

use super::severity;

/// The card's width range, in pixels.
const MIN_WIDTH: f64 = 240.0;
const MAX_WIDTH: f64 = 560.0;
/// The least height a card is given, even with little room.
const MIN_HEIGHT: f64 = 60.0;
/// Below this much room under the line the card flips above it, if that has more.
const COMFORTABLE_HEIGHT: f64 = 180.0;
/// Space kept clear at the editor's edges.
const EDGE: f64 = 12.0;
/// Space between the card and the line it is about.
const GAP: f64 = 4.0;
/// Documentation longer than this is cut.
const MAX_DOC_CHARS: usize = 1500;

#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticLine {
    pub rule: String,
    pub message: String,
    pub severity: DiagnosticSeverity,
}

/// Where the card attaches, in pixels from the editor's top-left corner.
#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub x: f64,
    pub line_top: f64,
    pub line_height: f64,
}

/// The size of the editor the card must fit in.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HoverCardState {
    left: f64,
    /// Distance from the editor's top edge, or from its bottom edge when the
    /// card opens above the line.
    top: Option<f64>,
    bottom: Option<f64>,
    max_width: f64,
    max_height: f64,
    diagnostics: Vec<DiagnosticLine>,
    signature_html: Option<String>,
    docstring: Option<String>,
}

impl HoverCardState {
    /// Places a card for `diagnostics` and `hover` below the anchored line, or
    /// above it when that has clearly more room.
    pub fn new(
        anchor: Anchor,
        bounds: Bounds,
        diagnostics: Vec<DiagnosticLine>,
        hover: Option<HoverInfo>,
    ) -> Self {
        let line_bottom = anchor.line_top + anchor.line_height;
        let room_below = bounds.height - line_bottom - EDGE - GAP;
        let room_above = anchor.line_top - EDGE - GAP;
        let is_above = room_below < COMFORTABLE_HEIGHT && room_above > room_below;
        let left = anchor.x.min(bounds.width - EDGE - MIN_WIDTH).max(EDGE);

        let (signature_html, docstring) = match hover {
            Some(hover) => (
                Some(signature_html(&hover.signature)),
                hover
                    .docstring
                    .map(|doc| truncate_chars(doc, MAX_DOC_CHARS)),
            ),
            None => (None, None),
        };

        Self {
            left,
            top: (!is_above).then_some(line_bottom + GAP),
            bottom: is_above.then_some(bounds.height - anchor.line_top + GAP),
            max_width: (bounds.width - left - EDGE).clamp(MIN_WIDTH.min(bounds.width), MAX_WIDTH),
            max_height: (if is_above { room_above } else { room_below }).max(MIN_HEIGHT),
            diagnostics,
            signature_html,
            docstring,
        }
    }

    fn style(&self) -> String {
        let mut style = format!(
            "left:{}px;max-width:{}px;max-height:{}px;",
            self.left, self.max_width, self.max_height
        );
        if let Some(top) = self.top {
            style.push_str(&format!("top:{top}px;"));
        }
        if let Some(bottom) = self.bottom {
            style.push_str(&format!("bottom:{bottom}px;"));
        }
        style
    }
}

#[component]
pub fn HoverCard(state: HoverCardState) -> impl IntoView {
    let style = state.style();
    view! {
        <div class="hover-card" style=style>
            {state
                .diagnostics
                .into_iter()
                .map(|diagnostic| {
                    view! {
                        <div class=format!(
                            "hover-card-diagnostic {}",
                            severity::state_class(diagnostic.severity),
                        )>
                            <span class="hover-card-icon">
                                {severity::icon(diagnostic.severity)}
                            </span>
                            <div class="hover-card-message">
                                <span class="hover-card-rule">"[" {diagnostic.rule} "]"</span>
                                " "
                                {diagnostic.message}
                            </div>
                        </div>
                    }
                })
                .collect_view()}
            {state
                .signature_html
                .map(|html| {
                    view! {
                        <div class="hover-card-signature">
                            <code inner_html=html></code>
                        </div>
                    }
                })}
            {state.docstring.map(|doc| view! { <div class="hover-card-doc">{doc}</div> })}
        </div>
    }
}

/// A signature coloured like code. A leading `(kind)`, such as `(variable)`, is
/// set apart in the muted style.
fn signature_html(signature: &str) -> String {
    match signature
        .strip_prefix('(')
        .and_then(|_| signature.find(") "))
    {
        Some(end) => format!(
            "<span class=\"hover-card-kind\">{}</span> {}",
            &signature[..=end],
            highlight::to_html(&signature[end + 2..], &[], &[])
        ),
        None => highlight::to_html(signature, &[], &[]),
    }
}

/// Cuts `text` to `max` characters, ending in `…` if it was cut.
fn truncate_chars(text: String, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}…", text[..end].trim_end()),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDS: Bounds = Bounds {
        width: 800.0,
        height: 600.0,
    };

    fn card_at(x: f64, line_top: f64) -> HoverCardState {
        let anchor = Anchor {
            x,
            line_top,
            line_height: 21.0,
        };
        HoverCardState::new(anchor, BOUNDS, Vec::new(), None)
    }

    #[test]
    fn the_card_opens_below_a_line_with_room() {
        let card = card_at(100.0, 100.0);
        assert_eq!(card.top, Some(125.0));
        assert_eq!(card.bottom, None);
    }

    #[test]
    fn the_card_flips_above_a_line_near_the_bottom() {
        let card = card_at(100.0, 560.0);
        assert_eq!(card.top, None);
        assert_eq!(card.bottom, Some(44.0));
    }

    #[test]
    fn the_card_stays_below_when_above_has_no_more_room() {
        let card = card_at(100.0, 10.0);
        assert!(card.top.is_some());
    }

    #[test]
    fn the_card_stays_inside_the_editor_horizontally() {
        assert_eq!(card_at(-50.0, 100.0).left, EDGE);
        assert_eq!(card_at(790.0, 100.0).left, BOUNDS.width - EDGE - MIN_WIDTH);
    }

    #[test]
    fn signatures_set_the_kind_apart() {
        let html = signature_html("(variable) x: int");
        assert!(
            html.starts_with("<span class=\"hover-card-kind\">(variable)</span> "),
            "{html}"
        );
        assert!(!signature_html("def f() -> int").contains("hover-card-kind"));
    }

    #[test]
    fn long_documentation_is_cut_on_a_character_boundary() {
        assert_eq!(truncate_chars("héllo wörld".to_string(), 5), "héllo…");
        assert_eq!(truncate_chars("short".to_string(), 10), "short");
    }
}
