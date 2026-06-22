//! Defines the command-line interface for difftree.

use crate::sort;
use clap::{Parser, Subcommand, ValueEnum};
use std::fmt;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
#[command(override_usage = "difftree [OPTIONS] [PATH]")]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[command(flatten)]
    pub view: ViewArgs,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(hide = true, visible_alias = "i")]
    Interactive(InteractiveArgs),
}

#[derive(Parser, Debug, Default)]
pub struct ViewArgs {
    #[arg(default_value = ".")]
    pub path: PathBuf,
    #[arg(long, value_name = "WHEN", default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
    #[arg(long, value_name = "pretty|plain")]
    pub format: Option<FormatChoice>,
    #[arg(long)]
    pub json: bool,
    #[arg(long, alias = "no-git")]
    pub plain: bool,
    #[arg(long)]
    pub tree: bool,
    #[arg(long)]
    pub unstaged: bool,
    #[arg(long)]
    pub all: bool,
    #[arg(long, value_name = "A..B")]
    pub range: Option<String>,
    #[arg(long, value_name = "REF")]
    pub against: Option<String>,
    #[arg(long, default_value = "color,bar,badge")]
    pub heat: String,
    #[arg(long, default_value_t = MarkScheme::Symbol)]
    pub marks: MarkScheme,
    #[arg(long)]
    pub show_ignored: bool,
    #[arg(long)]
    pub ignored: bool,
    #[arg(short = 'L', long, alias = "depth")]
    pub level: Option<usize>,
    #[arg(short = 'd', long)]
    pub dirs_only: bool,
    #[arg(short = 's', long)]
    pub size: bool,
    #[arg(short = 'p', long)]
    pub permissions: bool,
    #[arg(short = 'a', long, help = "Show all files, including hidden ones")]
    pub show_all: bool,
    #[arg(short = 'g', long)]
    pub gitignore: bool,
    #[arg(short = 'G', long)]
    pub git_status: bool,
    #[arg(long)]
    pub icons: bool,
    #[arg(long)]
    pub hyperlinks: bool,
    #[arg(short = 'P', long = "pattern")]
    pub include_pattern: Option<String>,
    #[arg(short = 'I', long = "ignore-pattern")]
    pub exclude_pattern: Option<String>,
    #[arg(long)]
    pub prune: bool,
    #[arg(long, alias = "dirsfirst")]
    pub dirs_first: bool,
    #[arg(long)]
    pub noreport: bool,
    #[arg(long)]
    pub filelimit: Option<usize>,
    #[arg(short = 'n', long = "no-color")]
    pub no_color: bool,
    #[arg(short = 'C', long = "force-color")]
    pub force_color: bool,
    #[arg(long, default_value_t = SortType::Name)]
    pub sort: SortType,
    #[arg(long)]
    pub case_sensitive: bool,
    #[arg(long)]
    pub natural_sort: bool,
    #[arg(short = 'r', long)]
    pub reverse: bool,
    #[arg(long)]
    pub dotfiles_first: bool,
}

#[derive(Parser, Debug)]
pub struct InteractiveArgs {
    #[arg(default_value = ".")]
    pub path: PathBuf,
    #[arg(short = 'a', long)]
    pub all: bool,
    #[arg(short = 'g', long)]
    pub gitignore: bool,
    #[arg(short = 'G', long)]
    pub git_status: bool,
    #[arg(long)]
    pub icons: bool,
    #[arg(short = 's', long)]
    pub size: bool,
    #[arg(short = 'p', long)]
    pub permissions: bool,
    #[arg(long, value_name = "LEVEL")]
    pub expand_level: Option<usize>,
    #[arg(long, default_value_t = SortType::Name)]
    pub sort: SortType,
    #[arg(long)]
    pub dirs_first: bool,
    #[arg(long)]
    pub case_sensitive: bool,
    #[arg(long)]
    pub natural_sort: bool,
    #[arg(short = 'r', long)]
    pub reverse: bool,
    #[arg(long)]
    pub dotfiles_first: bool,
}

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum SortType {
    #[default]
    Name,
    Size,
    Modified,
    Extension,
}
#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum ColorChoice {
    Always,
    #[default]
    Auto,
    Never,
}
#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum FormatChoice {
    Pretty,
    Plain,
}
#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum MarkScheme {
    #[default]
    Symbol,
    Letter,
    Xy,
}

impl From<SortType> for sort::SortType {
    fn from(sort_type: SortType) -> Self {
        match sort_type {
            SortType::Name => sort::SortType::Name,
            SortType::Size => sort::SortType::Size,
            SortType::Modified => sort::SortType::Modified,
            SortType::Extension => sort::SortType::Extension,
        }
    }
}
impl ViewArgs {
    pub fn to_sort_options(&self) -> sort::SortOptions {
        sort::SortOptions {
            sort_type: self.sort.into(),
            directories_first: self.dirs_first,
            case_sensitive: self.case_sensitive,
            natural_sort: self.natural_sort,
            reverse: self.reverse,
            dotfiles_first: self.dotfiles_first,
        }
    }
}
impl InteractiveArgs {
    pub fn to_sort_options(&self) -> sort::SortOptions {
        sort::SortOptions {
            sort_type: self.sort.into(),
            directories_first: self.dirs_first,
            case_sensitive: self.case_sensitive,
            natural_sort: self.natural_sort,
            reverse: self.reverse,
            dotfiles_first: self.dotfiles_first,
        }
    }
}
impl fmt::Display for SortType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_possible_value().expect("no values are skipped").get_name().fmt(f)
    }
}
impl fmt::Display for ColorChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_possible_value().expect("no values are skipped").get_name().fmt(f)
    }
}
impl fmt::Display for FormatChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_possible_value().expect("no values are skipped").get_name().fmt(f)
    }
}
impl fmt::Display for MarkScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_possible_value().expect("no values are skipped").get_name().fmt(f)
    }
}
