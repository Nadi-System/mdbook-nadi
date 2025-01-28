use anyhow::Context;
use pulldown_cmark::Event;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use crate::output::{clipped_from_stdout, output_handler, output_verbose};

pub type CodeHandler = fn(&str, &str, &Path) -> Result<Vec<Event<'static>>, anyhow::Error>;

pub fn nadi_code_args(mark: &str) -> Option<(CodeHandler, String)> {
    // only run for ones with "run" in it
    mark.split_once(" run").and_then(|(p, a)| match p.trim() {
        "table" => Some((run_table as CodeHandler, a.to_string())),
        "task" => Some((run_task as CodeHandler, a.to_string())),
        "stp" | "string-template" => Some((run_template as CodeHandler, a.to_string())),
        _ => None,
    })
}

pub fn run_task(task: &str, args: &str, pwd: &Path) -> anyhow::Result<Vec<Event<'static>>> {
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

    let output_fmt = args.trim().split(' ').next().unwrap_or("verbose");
    let handler = output_handler(output_fmt);
    if out.status.success() {
        let response = String::from_utf8_lossy(&strip_ansi_escapes::strip(&clipped_from_stdout(
            &out.stdout,
        )))
        .to_string();
        Ok(handler(vec![Event::Text("Results:".into())], response, pwd))
    } else {
        let error = String::from_utf8_lossy(&out.stderr);
        Ok(output_verbose(
            vec![Event::Text("*Error*:".into())],
            error.trim().to_string(),
            pwd,
        ))
    }
}

pub fn run_template(templ: &str, args: &str, pwd: &Path) -> anyhow::Result<Vec<Event<'static>>> {
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
        Ok(txt) => Ok(output_verbose(
            vec![Event::Text(format!("Results (with: {args}):").into())],
            txt,
            pwd,
        )),
        Err(e) => Ok(output_verbose(
            vec![Event::Text("*Error*:".into())],
            e.to_string(),
            pwd,
        )),
    }
}

pub fn run_table(table: &str, args: &str, pwd: &Path) -> anyhow::Result<Vec<Event<'static>>> {
    // TODO proper temp random file
    let task_path = "/tmp/task-temp.tasks";
    let mut output = std::fs::File::create(task_path)?;
    let mut table_contents = String::new();
    for line in table.split('\n') {
        if let Some(l) = line.strip_prefix('!') {
            writeln!(output, "{}", l)?;
        } else {
            table_contents.push_str(line);
            table_contents.push('\n');
        }
    }

    let mut output_fmt = "markdown";
    let mut targs = String::new();
    if let Some((fmt, mut a)) = args.trim().split_once(' ') {
        output_fmt = fmt;
        a = a.trim();
        if !a.is_empty() {
            targs.push(',');
            targs.push_str(a);
        }
    }

    writeln!(output, "network table_to_{}(template=\"", output_fmt)?;
    write!(output, "{}", table_contents)?;
    writeln!(output, "\"{})", targs)?;

    let out = Command::new("nadi")
        .arg(task_path)
        .current_dir(pwd)
        .output()
        .context("Could not run nadi command")?;

    let handler = output_handler(output_fmt);
    if out.status.success() {
        let response = clipped_from_stdout(&out.stdout);
        Ok(handler(vec![Event::Text("Results:".into())], response, pwd))
    } else {
        let error = String::from_utf8_lossy(&out.stderr);
        Ok(output_verbose(
            vec![Event::Text("*Error*:".into())],
            error.trim().to_string(),
            pwd,
        ))
    }
}
