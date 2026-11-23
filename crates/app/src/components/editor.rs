//! The code editor: a textarea over a coloured copy of its text, with smart
//! indentation and hover cards.

use std::time::Duration;

use leptos::{ev, prelude::*};
use pyde_analysis::highlight;
use pyde_analysis::protocol::{DiagnosticInfo, HoverInfo};
use web_sys::HtmlTextAreaElement;

use super::hover_card::{Anchor, Bounds, DiagnosticLine, HoverCard, HoverCardState};
use crate::editing::indent;
use crate::editing::layout::{self, Metrics};
use crate::editing::textarea;
use crate::state::Checker;

/// How long the pointer must rest on a token before a card appears. Moving
/// between tokens while a card is open swaps it straight away.
const SHOW_DELAY: Duration = Duration::from_millis(250);
const MAX_DIAGNOSTICS_SHOWN: usize = 3;

/// What the current (or pending) card is about, as the byte range of its token.
type Target = (usize, usize);

/// Textarea editor bound to `code`. The textarea's own text is transparent, and a
/// coloured copy, with Ty's semantic tokens, diagnostics and dead-code dimming,
/// sits behind it and follows its scrolling.
#[component]
pub fn Editor(
    code: RwSignal<String>,
    checker: Checker,
    textarea_ref: NodeRef<leptos::html::Textarea>,
) -> impl IntoView {
    let scroll = RwSignal::new((0, 0));
    let hover_card = RwSignal::new(None::<HoverCardState>);

    let metrics = StoredValue::new(None::<Metrics>);
    let target = StoredValue::new(None::<Target>);
    // Bumped whenever the pointer moves to something new, so that a card waiting
    // out SHOW_DELAY, or an answer still on its way, is dropped as out of date.
    let generation = StoredValue::new(0u64);

    let hide_card = move || {
        generation.update_value(|g| *g += 1);
        if hover_card.with_untracked(Option::is_some) {
            hover_card.set(None);
        }
    };
    let hide = move || {
        target.set_value(None);
        hide_card();
    };

    // Enter, Backspace and Tab are smart about indentation. Tab never moves focus
    // out of the editor.
    let on_keydown = move |e: ev::KeyboardEvent| {
        if !matches!(e.key().as_str(), "Shift" | "Control" | "Alt" | "Meta") {
            hide();
        }
        let shortcut = e.ctrl_key() || e.meta_key() || e.alt_key();
        if shortcut || e.is_composing() {
            return;
        }
        let key = e.key();
        if !matches!(key.as_str(), "Enter" | "Backspace" | "Tab") {
            return;
        }

        let area = event_target::<HtmlTextAreaElement>(&e);
        let text = area.value();
        let start = area.selection_start().ok().flatten().unwrap_or(0) as usize;
        let end = area.selection_end().ok().flatten().unwrap_or(start as u32) as usize;
        let (start, end) = (
            layout::utf16_to_byte(&text, start),
            layout::utf16_to_byte(&text, end),
        );

        let edit = match key.as_str() {
            "Enter" => Some(indent::enter(&text, start, end)),
            "Backspace" => indent::backspace(&text, start, end),
            _ => Some(indent::tab(&text, start, end, e.shift_key())),
        };
        // Without an edit, the browser's own key is right.
        let Some(edit) = edit else { return };
        e.prevent_default();
        let new_text = textarea::apply(&area, &edit);
        if code.with_untracked(|current| *current != new_text) {
            code.set(new_text);
        }
    };

    let on_scroll = move |e: ev::Event| {
        hide();
        let area = event_target::<HtmlTextAreaElement>(&e);
        scroll.set((area.scroll_left(), area.scroll_top()));
    };

    let on_mousemove = move |e: ev::MouseEvent| {
        let area = event_target::<HtmlTextAreaElement>(&e);
        let rect = area.get_bounding_client_rect();
        let rel_x = e.client_x() as f64 - rect.left();
        let rel_y = e.client_y() as f64 - rect.top();

        // The scrollbars are part of the textarea, but not part of the text.
        if rel_x >= area.client_width() as f64 || rel_y >= area.client_height() as f64 {
            return hide();
        }

        let m = metrics.get_value().unwrap_or_else(|| {
            let measured = layout::measure(&area).unwrap_or_default();
            metrics.set_value(Some(measured));
            measured
        });
        let text = code.get_untracked();
        let (scroll_left, scroll_top) = (area.scroll_left() as f64, area.scroll_top() as f64);
        let Some(hit) = layout::locate(&text, rel_x + scroll_left, rel_y + scroll_top, &m) else {
            return hide();
        };

        // Still on the word the card is about, or was asked about.
        if target
            .get_value()
            .is_some_and(|(start, end)| (start..end).contains(&hit.offset))
        {
            return;
        }

        let offset = hit.offset;
        let (lines, diagnostic_range) = checker
            .diagnostics
            .with_untracked(|all| diagnostics_at(all, offset));
        if hit.is_whitespace && lines.is_empty() {
            return hide();
        }

        let word = layout::word_range(&text, offset);
        target.set_value(Some(word));
        generation.update_value(|g| *g += 1);
        let asked_at = generation.get_value();
        let bounds = Bounds {
            width: rect.width(),
            height: rect.height(),
        };

        // Runs once the answer is in. Everything it needs was captured at the
        // moment of the mouse move, so a late answer can't mix with a newer view.
        let show = move |hover: Option<HoverInfo>| {
            if generation.get_value() != asked_at {
                return;
            }
            if lines.is_empty() && hover.is_none() {
                return hide_card();
            }

            let key = hover
                .as_ref()
                .map(|h| (h.start_byte, h.end_byte))
                .or(diagnostic_range)
                .filter(|(start, end)| (*start..*end).contains(&offset))
                .unwrap_or(word);
            target.set_value(Some(key));

            // Anchor to the start of the token, unless it began on an earlier line.
            let anchor = match &hover {
                Some(h)
                    if text
                        .get(h.start_byte..offset)
                        .is_some_and(|s| !s.contains('\n')) =>
                {
                    h.start_byte
                }
                _ => offset,
            };
            let (x, y) = layout::position_of(&text, anchor, &m);
            let anchor = Anchor {
                x: x - scroll_left,
                line_top: y - scroll_top,
                line_height: m.line_height,
            };
            let state = HoverCardState::new(anchor, bounds, lines.clone(), hover);

            if hover_card.with_untracked(Option::is_some) {
                if hover_card.with_untracked(|card| card.as_ref() != Some(&state)) {
                    hover_card.set(Some(state));
                }
            } else {
                generation.update_value(|g| *g += 1);
                let pending = generation.get_value();
                set_timeout(
                    move || {
                        if generation.get_value() == pending {
                            hover_card.set(Some(state));
                        }
                    },
                    SHOW_DELAY,
                );
            }
        };

        if hit.is_whitespace {
            show(None);
        } else {
            checker.hover(offset, show);
        }
    };

    view! {
        <div class="editor" on:mouseleave=move |_| hide()>
            <pre class="editor-colours" aria-hidden="true">
                <code
                    style:transform=move || {
                        let (left, top) = scroll.get();
                        format!("translate({}px, {}px)", -left, -top)
                    }
                    inner_html=move || highlight::to_html(
                        &code.get(),
                        &checker.diagnostics.get(),
                        &checker.dead_code.get(),
                    )
                ></code>
            </pre>
            <textarea
                node_ref=textarea_ref
                class="editor-input"
                spellcheck="false"
                wrap="off"
                prop:value=move || code.get()
                on:input=move |e| {
                    hide();
                    code.set(event_target_value(&e));
                }
                on:keydown=on_keydown
                on:scroll=on_scroll
                on:mouseenter=move |_| metrics.set_value(None)
                on:mousemove=on_mousemove
            ></textarea>
            {move || hover_card.get().map(|state| view! { <HoverCard state=state /> })}
        </div>
    }
}

/// The diagnostics covering byte `offset`, a few at most, and the range of the
/// first one.
fn diagnostics_at(
    diagnostics: &[DiagnosticInfo],
    offset: usize,
) -> (Vec<DiagnosticLine>, Option<Target>) {
    let mut covering = diagnostics
        .iter()
        .filter(|d| (d.start_byte..d.end_byte).contains(&offset))
        .peekable();
    let range = covering.peek().map(|d| (d.start_byte, d.end_byte));
    let lines = covering
        .take(MAX_DIAGNOSTICS_SHOWN)
        .map(|d| DiagnosticLine {
            rule: d.rule.clone(),
            message: d.message.clone(),
            severity: d.severity,
        })
        .collect();
    (lines, range)
}
