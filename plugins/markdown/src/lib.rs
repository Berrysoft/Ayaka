use gal_bindings::*;
use pulldown_cmark::{html::push_html, Parser};

#[export]
fn plugin_type() -> PluginType {
    PluginType::Action
}

#[export]
fn process_action(frontend: FrontendType, mut action: Action) -> Action {
    match frontend {
        FrontendType::Html => {
            let line = action
                .line
                .into_iter()
                .map(|s| s.into_string())
                .collect::<Vec<_>>()
                .concat();
            let parser = Parser::new(&line);
            let mut buffer = String::new();
            push_html(&mut buffer, parser);
            action.line = vec![ActionLine::Chars(buffer.trim().into())];
        }
        _ => {}
    }
    action
}
