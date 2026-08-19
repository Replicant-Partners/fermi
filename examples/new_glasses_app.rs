//! Generate the AIUI glasses shells from `SHELL_SPECS`.
//!
//! ```text
//! cargo run --example new_glasses_app             # write every registered shell
//! cargo run --example new_glasses_app -- --check   # report drift, write nothing
//! cargo run --example new_glasses_app -- <agent>   # one shell
//! ```
//!
//! `--check` is what CI would run if the parity test were not already doing it
//! in-process. It exists for the human case: knowing *what* drifted before
//! deciding whether the template or the file on disk is the thing that is wrong.
//! The parity test only says they disagree.
//!
//! Regeneration is deliberately destructive of hand edits, and that is the
//! point. The generated app is the display surface for a trust boundary; a local
//! edit that removed the fail-closed unstamped check would be silent, because
//! the shell would still render markers. Making the edit disappear is a worse
//! outcome than losing it.

use fermi::glasses_shell::{app_dir, render, GeneratedFile, ShellSpec, SHELL_SPECS};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = args.iter().any(|a| a == "--check");
    let only: Option<&str> = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(String::as_str);

    let selected: Vec<&ShellSpec> = SHELL_SPECS
        .iter()
        .filter(|s| only.is_none_or(|id| s.agent_id == id))
        .collect();

    if selected.is_empty() {
        eprintln!(
            "no registered shell matches `{}`. Registered: {}",
            only.unwrap_or(""),
            SHELL_SPECS
                .iter()
                .map(|s| s.agent_id)
                .collect::<Vec<_>>()
                .join(", ")
        );
        eprintln!(
            "\nA new shell is a `ShellSpec` in src/glasses_shell.rs, not a new \
             directory. An app that is not registered is not checked, and an \
             unchecked app is the hand-written copy the generator exists to \
             prevent."
        );
        std::process::exit(2);
    }

    let mut drifted = 0usize;
    let mut written = 0usize;

    for spec in selected {
        let dir = app_dir(spec);
        println!("\n{} -> {dir}", spec.agent_id);

        for file in render(spec) {
            let path = format!("{dir}/{}", file.path);
            let existing = std::fs::read_to_string(&path).ok();

            match &existing {
                Some(current) if *current == file.contents => {
                    println!("  = {}", file.path);
                    continue;
                }
                Some(_) => {
                    drifted += 1;
                    println!("  ~ {} (differs from the generator)", file.path);
                }
                None => {
                    drifted += 1;
                    println!("  + {} (absent)", file.path);
                }
            }

            if !check {
                write(&path, &file);
                written += 1;
            }
        }
    }

    println!();
    if check {
        if drifted == 0 {
            println!("clean: every generated file matches src/glasses_shell.rs");
        } else {
            println!(
                "{drifted} file(s) differ from the generator.\n\
                 \n\
                 Decide which is wrong before regenerating. If the file on disk \
                 is right, the template lost something and regenerating would \
                 delete it."
            );
            std::process::exit(1);
        }
    } else {
        println!("wrote {written} file(s)");
    }
}

fn write(path: &str, file: &GeneratedFile) {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("mkdir {parent:?}: {e}"));
    }
    std::fs::write(path, &file.contents).unwrap_or_else(|e| panic!("write {path}: {e}"));
}
