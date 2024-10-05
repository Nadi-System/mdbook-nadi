use crate::cmark_events::event_to_static;
use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use std::path::Path;

pub type OutputHandler = fn(Vec<Event<'static>>, String, &Path) -> Vec<Event<'static>>;

pub fn output_handler(fmt: &str) -> OutputHandler {
    match fmt {
        "markdown" => output_markdown as OutputHandler,
        "verbose" | "txt" | "text" => output_verbose as OutputHandler,
        "image" => output_image as OutputHandler,
        "file" => output_file as OutputHandler,
        _ => output_verbose as OutputHandler,
    }
}

pub fn output_verbose(
    mut pre: Vec<Event<'static>>,
    text: String,
    _pwd: &Path,
) -> Vec<Event<'static>> {
    pre.extend(vec![
        Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced("output".into()))),
        Event::Text(text.into()),
        Event::End(TagEnd::CodeBlock),
    ]);
    pre
}

pub fn output_image(
    mut pre: Vec<Event<'static>>,
    text: String,
    _pwd: &Path,
) -> Vec<Event<'static>> {
    let img = Tag::Image {
        link_type: pulldown_cmark::LinkType::Reference,
        dest_url: text.into(),
        title: String::new().into(),
        id: String::new().into(),
    };
    pre.extend(vec![
        Event::HardBreak,
        Event::Start(img),
        Event::End(TagEnd::Image),
        Event::HardBreak,
    ]);
    pre
}

pub fn output_file(mut pre: Vec<Event<'static>>, text: String, pwd: &Path) -> Vec<Event<'static>> {
    pre.push(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(
        "output".into(),
    ))));
    match std::fs::read_to_string(pwd.join(text)) {
        Ok(text) => pre.push(Event::Text(text.into())),
        Err(e) => pre.push(Event::Text(e.to_string().into())),
    };
    pre.push(Event::End(TagEnd::CodeBlock));
    pre
}

pub fn output_markdown(
    mut pre: Vec<Event<'static>>,
    text: String,
    _pwd: &Path,
) -> Vec<Event<'static>> {
    let mut opts = pulldown_cmark::Options::empty();
    opts.insert(pulldown_cmark::Options::ENABLE_TABLES);
    opts.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
    opts.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    opts.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
    opts.insert(pulldown_cmark::Options::ENABLE_HEADING_ATTRIBUTES);

    pre.push(Event::HardBreak);
    for e in pulldown_cmark::Parser::new_ext(&text, opts) {
        pre.push(event_to_static(e));
    }
    pre.push(Event::HardBreak);
    pre
}

pub fn clipped_from_stdout(output: &[u8]) -> String {
    let response = String::from_utf8_lossy(output);
    let mut parts = response.trim().split("----8<----");
    // optionally maybe we can just use the mdbook syntax to hide the line between the clip parts.
    let first = parts.next().unwrap_or_default();
    let parts: Vec<&str> = parts
        .enumerate()
        .filter_map(|(i, s)| if i % 2 == 0 { Some(s.trim()) } else { None })
        .collect();
    if parts.is_empty() {
        first.to_string()
    } else {
        parts.join("\n")
    }
}
