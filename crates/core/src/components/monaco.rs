use leptos::*;
use monaco::api::{CodeEditor, CreateEditorOptions};

#[component]
pub fn MonacoEditor(cx: Scope) -> impl IntoView {
    let container_ref = create_node_ref::<html::Div>();
}