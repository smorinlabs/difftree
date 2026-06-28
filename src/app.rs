//! Defines the command-line interface for difftree.

use crate::sort;
use clap::{Parser, Subcommand, ValueEnum};
use std::fmt;
use std::path::PathBuf;

const STATUS_KEY_HELP: &str = r#"Status key:
  Row format: <tree connector> <status mark> <path> +added -deleted
  Directories show a rollup: (<changed files> files, +added -deleted)

  --marks=symbol: ● staged, ○ unstaged, ◐ staged+unstaged, ? untracked,
                  ↻ renamed, ⧉ copied, × deleted, ◆ typechanged,
                  ‼ conflicted, ⚠ unreadable, ! ignored
  --marks=letter: S staged, M unstaged, B staged+unstaged, ? untracked,
                  R renamed, C copied, D deleted, T typechanged,
                  U conflicted, E unreadable, I ignored
  --marks=xy:     M_ staged, _M unstaged, MM staged+unstaged, ?? untracked,
                  R_ renamed, C_ copied, D_ deleted, T_ typechanged,
                  UU conflicted, E? unreadable, !! ignored
                  (_ means a space; clean files have a blank mark)

Definitions:
  staged: change is in the git index.
  unstaged: tracked worktree change is not staged.
  staged+unstaged: the same path has both staged and unstaged changes.
  untracked: path is not tracked by git.
  renamed: tracked path moved or renamed; shown as old -> new.
  copied: new path was detected as a copy; shown as source => copy.
  deleted: tracked path was removed.
  typechanged: tracked object kind changed, such as file <-> symlink or submodule.
  conflicted: unmerged path during merge/rebase/cherry-pick.
  unreadable: path exists but difftree could not read it.
  ignored: path matches git ignore rules when ignored entries are shown.
  clean: unchanged path shown in all-files views.

Colors when enabled: staged green, unstaged yellow, staged+unstaged cyan,
  untracked magenta, renamed blue, copied bright blue, deleted red,
  typechanged cyan, conflicted bright red, unreadable bright yellow,
  ignored gray. Churn is +added green and -deleted red; filenames follow
  LS_COLORS."#;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
#[command(override_usage = "difftree [OPTIONS] [PATH]")]
#[command(after_help = STATUS_KEY_HELP)]
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
    #[arg(long, alias = "cached")]
    pub staged: bool,
    #[arg(long)]
    pub unstaged: bool,
    #[arg(long)]
    pub uncommitted: bool,
    #[arg(long, alias = "tree")]
    pub all: bool,
    #[arg(long, value_name = "A..B")]
    pub range: Option<String>,
    #[arg(long, value_name = "REF")]
    pub against: Option<String>,
    #[arg(
        long,
        value_name = "REF",
        num_args = 0..=1,
        require_equals = true,
        conflicts_with_all = ["range", "against", "staged", "unstaged", "uncommitted"]
    )]
    pub pr: Option<Option<String>>,
    #[arg(long = "pr-base", value_name = "REF", requires = "pr")]
    pub pr_base: Option<String>,
    #[arg(long, requires = "pr")]
    pub committed: bool,
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
