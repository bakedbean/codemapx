//! codemapx CLI.

use std::{fmt::Display, path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use codemapx::{collect::collect, git::Git, load, store, tui};

#[derive(Parser)]
#[command(name = "codemapx", about = "A navigable map of the changes on a branch", args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
    /// Worktree to view (default: current directory)
    path: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Diff against the merge-base and write facts.json; prints the map dir
    Collect {
        path: Option<PathBuf>,
        /// Base ref (default: origin/main, then main)
        #[arg(long)]
        base: Option<String>,
    },
    /// Check annotations.json against facts.json
    Validate { path: Option<PathBuf> },
    /// Write the map as one self-contained HTML page
    Html {
        path: Option<PathBuf>,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Open the map in the terminal (the default)
    View {
        path: Option<PathBuf>,
        /// Print one frame for <PATH> [OUTLINE_IDX] instead of opening the TUI
        #[arg(long, num_args = 1..=2, value_names = ["PATH", "OUTLINE_IDX"])]
        snapshot: Vec<String>,
        #[arg(long, default_value_t = 180)]
        width: u16,
        #[arg(long, default_value_t = 52)]
        height: u16,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.cmd.unwrap_or(Cmd::View { path: cli.path, snapshot: vec![], width: 180, height: 52 }) {
        Cmd::Collect { path, base } => cmd_collect(path, base.as_deref()),
        Cmd::Validate { path } => cmd_validate(path),
        Cmd::Html { path, out } => cmd_html(path, &out),
        Cmd::View { path, snapshot, width, height } => cmd_view(path, &snapshot, width, height),
    }
}

fn fail(code: u8, msg: impl Display) -> ExitCode {
    eprintln!("codemapx: {msg}");
    ExitCode::from(code)
}

fn open(path: Option<PathBuf>) -> Result<Git, String> {
    Git::open(&path.unwrap_or_else(|| PathBuf::from(".")))
}

fn cmd_collect(path: Option<PathBuf>, base: Option<&str>) -> ExitCode {
    let facts = match open(path).and_then(|g| collect(&g, base)) {
        Ok(f) => f,
        Err(e) => return fail(2, e),
    };
    for w in &facts.warnings {
        eprintln!("warning: {w}");
    }
    match store::save_facts(&store::state_root(), &facts) {
        Ok(dir) => {
            println!("{}", dir.display());
            ExitCode::SUCCESS
        }
        Err(e) => fail(2, e),
    }
}

fn cmd_validate(path: Option<PathBuf>) -> ExitCode {
    let git = match open(path) {
        Ok(g) => g,
        Err(e) => return fail(2, e),
    };
    let loaded = match load::load(&git, true) {
        Ok(l) => l,
        Err(e) => return fail(1, e),
    };
    let stale = format!("annotations.json: written for {}, facts are at {} (stale; update it)", loaded.annotations.head, loaded.facts.head);
    let problems = match load::merge(&git, &loaded) {
        Ok(m) if m.annotations_stale => vec![stale],
        Ok(_) => vec![],
        Err(mut p) => {
            if loaded.annotations.head != loaded.facts.head {
                p.push(stale);
            }
            p
        }
    };
    let head = git.head().unwrap_or_default();
    if loaded.facts.head != head {
        eprintln!("note: facts are for {}, HEAD is {}; run codemapx collect", short(&loaded.facts.head), short(&head));
    }
    for p in &problems {
        println!("{p}");
    }
    println!("{} problem(s)", problems.len());
    if problems.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

fn cmd_view(path: Option<PathBuf>, snapshot: &[String], width: u16, height: u16) -> ExitCode {
    let git = match open(path) {
        Ok(g) => g,
        Err(e) => return fail(2, e),
    };
    let loaded = match load::load(&git, false) {
        Ok(l) => l,
        Err(e) => return fail(1, e),
    };
    let map = match load::merge(&git, &loaded) {
        Ok(m) => m,
        Err(ps) => {
            for p in &ps {
                eprintln!("{p}");
            }
            return fail(1, format!("{} problem(s); run codemapx validate", ps.len()));
        }
    };
    let mut app = tui::App::new(map, git.root().to_path_buf());
    let head = git.head().unwrap_or_default();
    if loaded.facts.head != head {
        app.behind = Some(git.count_between(&loaded.facts.head, &head).unwrap_or(0));
    }
    if let Some(id) = snapshot.first() {
        match app.map.card_index(id) {
            Some(i) => app.select(i),
            None => return fail(1, format!("{id}: no such card")),
        }
        if let Some(k) = snapshot.get(1).and_then(|k| k.parse::<isize>().ok()) {
            app.focus = tui::Pane::Inside;
            app.move_in(k);
        }
        println!("{}", tui::snapshot(&mut app, width, height));
        return ExitCode::SUCCESS;
    }
    match tui::run(&mut app) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(1, e),
    }
}

fn cmd_html(path: Option<PathBuf>, out: &std::path::Path) -> ExitCode {
    let git = match open(path) {
        Ok(g) => g,
        Err(e) => return fail(2, e),
    };
    let map = match load::load(&git, false).map_err(|e| vec![e]).and_then(|l| load::merge(&git, &l)) {
        Ok(m) => m,
        Err(ps) => {
            for p in &ps {
                eprintln!("{p}");
            }
            return fail(1, "map is not valid; run codemapx validate");
        }
    };
    match std::fs::write(out, codemapx::html::render(&map)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(1, format!("{}: {e}", out.display())),
    }
}
