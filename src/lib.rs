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

fn is_task(mark: &str) -> bool {
    // only run for ones with "run" in it
    mark.contains("task run")
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

    let mut task_script = String::new();
    let events = parser.try_fold(vec![], |mut acc, ref e| -> anyhow::Result<Vec<Event<'_>>> {
        use CodeBlockKind::*;
        use CowStr::*;
        use Event::*;
        use State::*;
        match (e, &mut state) {
            (Start(Tag::CodeBlock(Fenced(Borrowed(mark)))), None) if is_task(mark) => {
                acc.push(e.clone());
                state = Open;
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
                let results: String = run_task(&task_script, pwd)?;
                acc.push(Text("Results:\n".into()));
                acc.push(Start(Tag::CodeBlock(Fenced("output".into()))));
                acc.push(Text(results.into()));
                acc.push(End(TagEnd::CodeBlock));
            }
            _ => {
                acc.push(e.clone());
            }
        };
        Ok(acc)
    })?;
    Ok(cmark(events.iter(), &mut buf).map(|_| buf)?)
}

fn run_task(task: &str, pwd: &Path) -> anyhow::Result<String> {
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

    // let out = Command::new("pwd")
    //     .current_dir(pwd)
    //     .output()
    //     .context("Could not run nadi command")?;

    if out.status.success() {
        let response = String::from_utf8_lossy(&out.stdout);
        Ok(response.trim().to_string())
    } else {
        let error = String::from_utf8_lossy(&out.stderr);
        Ok(format!("****Error****\n{}", error.trim()))
    }
}
