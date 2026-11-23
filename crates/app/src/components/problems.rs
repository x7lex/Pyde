//! The Problems tab: the type checker's diagnostics as a list.

use leptos::prelude::*;
use pyde_analysis::protocol::DiagnosticInfo;

use super::severity;

/// The checker's diagnostics as a list. Clicking one calls `on_jump` with its
/// zero-based `(line, column)`.
#[component]
pub fn Problems(
    diagnostics: Signal<Vec<DiagnosticInfo>>,
    on_jump: Callback<(usize, usize)>,
) -> impl IntoView {
    view! {
        <div class="problems">
            {move || {
                let diagnostics = diagnostics.get();
                if diagnostics.is_empty() {
                    return view! { <p class="notice">"No problems detected by Ty."</p> }.into_any();
                }
                view! {
                    <div class="problems-list">
                        {diagnostics
                            .into_iter()
                            .map(|diagnostic| {
                                let position = (diagnostic.start_line, diagnostic.start_col);
                                view! {
                                    <div
                                        class=format!(
                                            "problem {}",
                                            severity::state_class(diagnostic.severity),
                                        )
                                        on:click=move |_| on_jump.run(position)
                                    >
                                        <div class="problem-header">
                                            <span class="problem-rule">
                                                {severity::icon(diagnostic.severity)} " [" {diagnostic.rule}
                                                "]"
                                            </span>
                                            <span class="problem-location">
                                                {format!("Line {}", diagnostic.start_line + 1)}
                                            </span>
                                        </div>
                                        <div class="problem-message">{diagnostic.message}</div>
                                        {diagnostic
                                            .help
                                            .map(|help| {
                                                view! { <div class="problem-help">"Tip: " {help}</div> }
                                            })}
                                    </div>
                                }
                            })
                            .collect_view()}
                    </div>
                }
                    .into_any()
            }}
        </div>
    }
}
