//! difftree CLI entry point.

mod app;
mod git;
mod icons;
mod sort;
mod tui;
mod utils;
mod view;

use app::{Args, ColorChoice, Commands, FormatChoice, MarkScheme};
use clap::Parser;
#[cfg(windows)]
use colored::control;
use difftree::{
    collect_all_files, collect_changes, collect_default_with_fallback, ComparisonMode,
    JsonRenderer, OutputFormat, Renderer, TerminalRenderer,
};
use lscolors::LsColors;

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    let _ = control::set_virtual_terminal(true);
    let args = Args::parse();
    let ls_colors = LsColors::from_env().unwrap_or_default();
    match &args.command {
        Some(Commands::Interactive(interactive_args)) => tui::run(interactive_args, &ls_colors),
        None => run_cli(&args, &ls_colors),
    }
}

fn run_cli(args: &Args, ls_colors: &LsColors) -> anyhow::Result<()> {
    let view_args = &args.view;
    if view_args.no_color || std::env::var_os("NO_COLOR").is_some() {
        colored::control::set_override(false);
    }
    if view_args.force_color {
        colored::control::set_override(true);
    }
    match view_args.color {
        ColorChoice::Always => colored::control::set_override(true),
        ColorChoice::Never => colored::control::set_override(false),
        ColorChoice::Auto => {}
    }

    let wants_plain_tree = view_args.plain
        || view_args.git_status
        || (!view_args.json
            && !view_args.all
            && !view_args.unstaged
            && !view_args.uncommitted
            && view_args.range.is_none()
            && view_args.against.is_none()
            && !view_args.ignored
            && !is_git_repo(&view_args.path));
    if wants_plain_tree {
        if !is_git_repo(&view_args.path) {
            eprintln!(
                "difftree: outside a git repository; showing plain tree (git features unavailable)"
            );
        }
        return view::run(view_args, ls_colors);
    }

    let mode = if let Some(range) = &view_args.range {
        ComparisonMode::Range { range: range.clone() }
    } else if let Some(reference) = &view_args.against {
        ComparisonMode::Against { reference: reference.clone() }
    } else if view_args.uncommitted {
        ComparisonMode::Uncommitted
    } else if view_args.unstaged {
        ComparisonMode::Unstaged
    } else {
        ComparisonMode::Staged
    };
    let explicit_mode = view_args.uncommitted
        || view_args.unstaged
        || view_args.staged
        || view_args.range.is_some()
        || view_args.against.is_some();
    let use_fallback = !explicit_mode && !view_args.all && !view_args.ignored;
    let walk = difftree::WalkOpts {
        all: view_args.show_all,
        gitignore: view_args.gitignore,
        level: view_args.level,
        dirs_only: view_args.dirs_only,
    };
    let tree = if view_args.all {
        // All-files view honors the same staged -> unstaged fallback as the bare
        // default when no explicit comparison flag is given (spec §3).
        let mut t = collect_all_files(&view_args.path, mode.clone(), walk)?;
        if !explicit_mode && t.as_ref().is_some_and(|i| i.summary.files_changed == 0) {
            let mut u = collect_all_files(&view_args.path, ComparisonMode::Unstaged, walk)?;
            if let Some(ui) = &mut u {
                ui.fallback = Some("No staged changes — showing unstaged changes".to_string());
            }
            if u.is_some() {
                t = u;
            }
        }
        t
    } else if use_fallback {
        collect_default_with_fallback(&view_args.path)?
    } else {
        collect_changes(&view_args.path, mode, true)?
    };
    let Some(tree) = tree else {
        return view::run(view_args, ls_colors);
    };
    if view_args.json {
        println!("{}", JsonRenderer.render(&tree)?);
    } else {
        let format = match view_args.format.unwrap_or(FormatChoice::Pretty) {
            FormatChoice::Pretty => OutputFormat::Pretty,
            FormatChoice::Plain => OutputFormat::Plain,
        };
        let marks = match view_args.marks {
            MarkScheme::Symbol => difftree::MarkScheme::Symbol,
            MarkScheme::Letter => difftree::MarkScheme::Letter,
            MarkScheme::Xy => difftree::MarkScheme::Xy,
        };
        print!("{}", TerminalRenderer { marks, format }.render(&tree)?);
    }
    Ok(())
}

fn is_git_repo(path: &std::path::Path) -> bool {
    git2::Repository::discover(path).is_ok()
}
