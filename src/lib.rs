use anyhow::{Context, Error};
use mdbook::book::{Book, BookItem};
use mdbook::preprocess::{Preprocessor, PreprocessorContext};

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag, TagEnd};
use pulldown_cmark_to_cmark::cmark;
use std::io::Write;
use std::path::Path;
use std::process::Command;

// use serde::Deserialize;

// #[derive(Debug, Deserialize)]
// struct NadiConfig {
//     network: Option<String>,
//     ignore: String,
// }

// impl Default for NadiConfig {
//     fn default() -> Self {
//         Self {
//             network: None,
//             ignore: "!".into(),
//         }
//     }
// }

/// A NADI preprocessor.
pub struct Nadi;

impl Nadi {
    pub fn new() -> Nadi {
        Nadi
    }
}

impl Preprocessor for Nadi {
    fn name(&self) -> &str {
        "nadi-preprocessor"
    }

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book, Error> {
        let src_dir = ctx.root.clone().join(&ctx.config.book.src);
        book.for_each_mut(|item| {
            let _ = process_book_item(item, &src_dir).map_err(|err| {
                eprintln!("{}", err);
            });
        });
        Ok(book)
    }

    fn supports_renderer(&self, renderer: &str) -> bool {
        renderer != "not-supported"
    }
}

fn process_book_item(item: &mut BookItem, pwd: &Path) -> anyhow::Result<()> {
    if let BookItem::Chapter(ref mut chapter) = item {
        chapter.content = run_chapter(&chapter.content, pwd)?;
    }
    Ok(())
}

type CodeHandler = fn(&str, &str, &Path) -> Result<Vec<Event<'static>>, Error>;

fn nadi_code_args(mark: &str) -> Option<(CodeHandler, String)> {
    // only run for ones with "run" in it
    mark.split_once(" run").and_then(|(p, a)| match p.trim() {
        "table" => Some((run_table as CodeHandler, a.to_string())),
        "task" => Some((run_task as CodeHandler, a.to_string())),
        "stp" | "string-template" => Some((run_template as CodeHandler, a.to_string())),
        _ => None,
    })
}

fn run_chapter(chap: &str, pwd: &Path) -> anyhow::Result<String> {
    enum State {
        None,
        Open,
        Gather,
    }

    let mut state = State::None;
    let mut buf = String::with_capacity(chap.len() * 2);
    let mut parser = mdbook::utils::new_cmark_parser(chap, false);
    let mut args = String::new();
    let mut handler: CodeHandler = run_task;

    let mut task_script = String::new();
    let events = parser.try_fold(vec![], |mut acc, ref e| -> anyhow::Result<Vec<Event<'_>>> {
        use CodeBlockKind::*;
        use CowStr::*;
        use Event::*;
        use State::*;
        match (e, &mut state) {
            (Start(Tag::CodeBlock(Fenced(Borrowed(mark)))), None) => {
                acc.push(e.clone());
                if let Some((h, a)) = nadi_code_args(mark) {
                    state = Open;
                    args = a;
                    handler = h;
                }
            }
            (Text(Borrowed(txt)), Open) => {
                acc.push(e.clone());
                task_script.clear();
                task_script.push_str(&txt);
                state = Gather;
            }
            (Text(Borrowed(txt)), Gather) => {
                task_script.push_str(&txt);
                acc.push(e.clone());
            }
            (End(TagEnd::CodeBlock), Gather) => {
                state = None;
                acc.push(e.clone());
                let response = handler(&task_script, &args, pwd)?;
                acc.extend(response);
            }
            _ => {
                acc.push(e.clone());
            }
        };
        Ok(acc)
    })?;
    Ok(cmark(events.iter(), &mut buf).map(|_| buf)?)
}

fn run_task(task: &str, _args: &str, pwd: &Path) -> anyhow::Result<Vec<Event<'static>>> {
    // TODO proper temp random file
    let task_path = "/tmp/task-temp.tasks";
    let mut output = std::fs::File::create(task_path)?;
    for line in task.split('\n') {
        writeln!(output, "{}", line.trim_start_matches('!'))?;
    }

    let out = Command::new("nadi")
        .arg(task_path)
        .current_dir(pwd)
        .output()
        .context("Could not run nadi command")?;

    if out.status.success() {
        let response = clipped_from_stdout(&out.stdout);
        Ok(output_to_block(
            vec![Event::Text("Results:".into())],
            response,
        ))
    } else {
        let error = String::from_utf8_lossy(&out.stderr);
        Ok(output_to_block(
            vec![Event::Text("*Error*:".into())],
            error.trim().to_string(),
        ))
    }
}

fn run_template(templ: &str, args: &str, _pwd: &Path) -> anyhow::Result<Vec<Event<'static>>> {
    let templ = string_template_plus::Template::parse_template(templ)?;
    let mut op = string_template_plus::RenderOptions::default();
    for kv in args.split(';') {
        if kv.is_empty() {
            continue;
        }
        let (k, v) = kv
            .split_once('=')
            .context("variables not in key=value pairs")?;
        op.variables
            .insert(k.trim().to_string(), v.trim().to_string());
    }
    match string_template_plus::Render::render(&templ, &op) {
        Ok(txt) => Ok(output_to_block(
            vec![Event::Text(format!("Results (with: {args}):").into())],
            txt,
        )),
        Err(e) => Ok(output_to_block(
            vec![Event::Text("*Error*:".into())],
            e.to_string(),
        )),
    }
}

fn run_table(table: &str, args: &str, pwd: &Path) -> anyhow::Result<Vec<Event<'static>>> {
    // TODO proper temp random file
    let task_path = "/tmp/task-temp.tasks";
    let mut output = std::fs::File::create(task_path)?;
    let mut table_contents = String::new();
    for line in table.split('\n') {
        if line.starts_with('!') {
            writeln!(output, "{}", &line[1..])?;
        } else {
            table_contents.push_str(line);
            table_contents.push('\n');
        }
    }

    let output_fmt = args.trim().split(' ').next().unwrap_or("markdown");
    writeln!(output, "network table_to_{}(template=\"", output_fmt)?;
    write!(output, "{}", table_contents)?;
    writeln!(output, "\")")?;

    let out = Command::new("nadi")
        .arg(task_path)
        .current_dir(pwd)
        .output()
        .context("Could not run nadi command")?;

    if out.status.success() {
        let response = clipped_from_stdout(&out.stdout);
        Ok(output_to_table(
            vec![Event::Text("Results:".into())],
            response,
        ))
    } else {
        let error = String::from_utf8_lossy(&out.stderr);
        Ok(output_to_block(
            vec![Event::Text("*Error*:".into())],
            error.trim().to_string(),
        ))
    }
}

fn clipped_from_stdout(output: &[u8]) -> String {
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

fn output_to_block(mut pre: Vec<Event<'static>>, text: String) -> Vec<Event<'static>> {
    pre.extend(vec![
        Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced("output".into()))),
        Event::Text(text.into()),
        Event::End(TagEnd::CodeBlock),
    ]);
    pre
}

fn output_to_table(mut pre: Vec<Event<'static>>, text: String) -> Vec<Event<'static>> {
    let mut opts = pulldown_cmark::Options::empty();
    opts.insert(pulldown_cmark::Options::ENABLE_TABLES);

    pre.push(Event::HardBreak);
    pre.push(Event::Start(Tag::Paragraph));
    for e in pulldown_cmark::Parser::new_ext(&text, opts) {
        pre.push(match e {
            Event::Start(Tag::Table(a)) => Event::Start(Tag::Table(a)),
            Event::End(TagEnd::Table) => Event::End(TagEnd::Table),
            Event::Start(Tag::TableHead) => Event::Start(Tag::TableHead),
            Event::End(TagEnd::TableHead) => Event::End(TagEnd::TableHead),
            Event::Start(Tag::TableRow) => Event::Start(Tag::TableRow),
            Event::End(TagEnd::TableRow) => Event::End(TagEnd::TableRow),
            Event::Start(Tag::TableCell) => Event::Start(Tag::TableCell),
            Event::End(TagEnd::TableCell) => Event::End(TagEnd::TableCell),
            Event::Text(t) => Event::Text(t.to_string().into()),
            // TODO add more tags into it, also test if we can only do
            // the variants with <'_> on it, and keep the others as
            // they are...
            _ => todo!(),
        });
    }
    pre.push(Event::End(TagEnd::Paragraph));
    pre
}
