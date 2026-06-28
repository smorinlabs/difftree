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
    collect_all_files, collect_all_files_default_with_fallback, collect_changes,
    collect_default_with_fallback, resolve_head_label, resolve_pr_base, ComparisonMode,
    JsonRenderer, OutputFormat, PrHeaderContext, Renderer, TerminalRenderer,
};
use lscolors::LsColors;

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    let _ = control::set_virtual_terminal(true);
    let args = Args::parse();
    let ls_colors = LsColors::from_env().unwrap_or_default();
    match &args.command {
        Some(Commands::Interactive(interactive_args)) if interactive_args.json => {
            let view_args = interactive_args.to_json_view_args();
            view::run_json(&view_args)
        }
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

    let explicit_mode = view_args.uncommitted
        || view_args.unstaged
        || view_args.staged
        || view_args.range.is_some()
        || view_args.against.is_some()
        || view_args.pr.is_some();
    let in_git_repo = is_git_repo(&view_args.path);
    let wants_plain_json = view_args.json
        && (view_args.plain || view_args.git_status || (!explicit_mode && !in_git_repo));
    if wants_plain_json {
        if !in_git_repo {
            eprintln!(
                "difftree: outside a git repository; showing plain tree (git features unavailable)"
            );
        }
        return view::run_json(view_args);
    }

    let wants_plain_tree = view_args.plain
        || view_args.git_status
        || (!view_args.json
            && !view_args.all
            && !view_args.staged
            && !view_args.unstaged
            && !view_args.uncommitted
            && view_args.range.is_none()
            && view_args.against.is_none()
            && view_args.pr.is_none()
            && !view_args.ignored
            && !in_git_repo);
    if wants_plain_tree {
        if !in_git_repo {
            eprintln!(
                "difftree: outside a git repository; showing plain tree (git features unavailable)"
            );
        }
        return view::run(view_args, ls_colors);
    }

    let pr_base = if let Some(pr_opt) = &view_args.pr {
        if pr_opt.is_some() && view_args.pr_base.is_some() {
            anyhow::bail!("difftree: use either --pr=<ref> or --pr-base <ref>, not both");
        }
        let base_override = pr_opt.as_deref().or(view_args.pr_base.as_deref());
        let base = resolve_pr_base(&view_args.path, base_override)?;
        if base.on_base {
            if view_args.committed {
                eprintln!(
                    "difftree: on base branch '{}'; no committed changes since base",
                    base.base_name
                );
            } else {
                eprintln!(
                    "difftree: on base branch '{}'; showing uncommitted changes only",
                    base.base_name
                );
            }
        }
        Some(base)
    } else {
        None
    };

    let mode = if let Some(base) = &pr_base {
        ComparisonMode::Pr { merge_base: base.merge_base.clone(), committed: view_args.committed }
    } else if let Some(range) = &view_args.range {
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
    let use_fallback = !explicit_mode && !view_args.all && !view_args.ignored;
    let walk = difftree::WalkOpts {
        all: view_args.show_all,
        gitignore: view_args.gitignore,
        level: view_args.level,
        dirs_only: view_args.dirs_only,
    };
    let tree = if view_args.all {
        if explicit_mode {
            collect_all_files(&view_args.path, mode, walk)?
        } else {
            collect_all_files_default_with_fallback(&view_args.path, walk)?
        }
    } else if use_fallback {
        collect_default_with_fallback(&view_args.path)?
    } else {
        let include_untracked = match &mode {
            ComparisonMode::Range { .. } => false,
            ComparisonMode::Pr { committed, .. } => !committed,
            _ => true,
        };
        collect_changes(&view_args.path, mode, include_untracked)?
    };
    let Some(tree) = tree else {
        if view_args.json && !explicit_mode {
            eprintln!(
                "difftree: outside a git repository; showing plain tree (git features unavailable)"
            );
            return view::run_json(view_args);
        }
        if view_args.json || explicit_mode {
            anyhow::bail!(
                "difftree: this command requires a git repository (use --plain for a plain tree, or run inside a repo)"
            );
        }
        if view_args.all {
            eprintln!(
                "difftree: outside a git repository; showing plain tree (git features unavailable)"
            );
        }
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
        let render_root =
            std::fs::canonicalize(&view_args.path).unwrap_or_else(|_| view_args.path.clone());
        let pr_header = pr_base
            .as_ref()
            .map(|base| {
                resolve_head_label(&view_args.path).map(|head_label| PrHeaderContext {
                    base_ref: base.base_ref.clone(),
                    head_label,
                    on_base: base.on_base,
                })
            })
            .transpose()?;
        print!(
            "{}",
            TerminalRenderer { marks, format, ls_colors, root: render_root, pr_header }
                .render(&tree)?
        );
    }
    Ok(())
}

fn is_git_repo(path: &std::path::Path) -> bool {
    git2::Repository::discover(path).is_ok()
}
