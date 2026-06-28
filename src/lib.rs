//! Core difftree library: serializable model, git-backed collection, and renderers.

use colored::Colorize;
use git2::{DiffFindOptions, DiffOptions, Repository};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: &str = "difftree.v2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComparisonMode {
    Staged,
    Unstaged,
    Uncommitted,
    Range { range: String },
    Against { reference: String },
    Pr { merge_base: String, committed: bool },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum View {
    BlastRadius,
    AllFiles,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Churn {
    pub added: usize,
    pub deleted: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Rollup {
    pub dirs_touched: usize,
    pub files_changed: usize,
    pub churn: Churn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChangeStatus {
    Staged,
    Unstaged,
    Both,
    Untracked,
    Renamed,
    Copied,
    Deleted,
    Typechanged,
    Conflicted,
    Unreadable,
    Ignored,
    Clean,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Typechanged,
    Conflicted,
    Unreadable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    #[serde(rename = "node_kind")]
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "kind")]
    pub change_kind: Option<ChangeKind>,
    pub status: ChangeStatus,
    pub churn: Churn,
    pub rollup: Rollup,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeKind {
    Directory,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeTree {
    pub schema_version: String,
    pub comparison: ComparisonMode,
    pub view: View,
    pub root: TreeNode,
    pub summary: Rollup,
    pub fallback: Option<String>,
}

pub trait Renderer {
    fn render(&self, tree: &ChangeTree) -> anyhow::Result<String>;
}

#[derive(Default)]
pub struct JsonRenderer;
impl Renderer for JsonRenderer {
    fn render(&self, tree: &ChangeTree) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(tree)?)
    }
}

#[derive(Debug, Clone)]
pub struct TerminalRenderer<'a> {
    pub marks: MarkScheme,
    pub format: OutputFormat,
    pub ls_colors: &'a lscolors::LsColors,
    pub root: PathBuf,
    pub pr_header: Option<PrHeaderContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrHeaderContext {
    pub base_ref: String,
    pub head_label: String,
    pub on_base: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkScheme {
    Symbol,
    Letter,
    Xy,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Pretty,
    Plain,
}

impl Renderer for TerminalRenderer<'_> {
    fn render(&self, tree: &ChangeTree) -> anyhow::Result<String> {
        let mut out = String::new();
        if let Some(f) = &tree.fallback {
            out.push_str(f);
            out.push('\n');
        }
        out.push_str(&self.render_header(tree));
        out.push('\n');
        out.push_str(&format!("{}\n", tree.root.name));
        for (idx, child) in tree.root.children.iter().enumerate() {
            self.node(&mut out, child, "", idx + 1 == tree.root.children.len());
        }
        let files_phrase = file_count_phrase(tree);
        out.push_str(&format!(
            "\n{} dirs touched · {} · {} {}\n",
            tree.summary.dirs_touched,
            files_phrase,
            add_str(tree.summary.churn.added),
            del_str(tree.summary.churn.deleted)
        ));
        Ok(out)
    }
}
impl TerminalRenderer<'_> {
    fn render_header(&self, tree: &ChangeTree) -> String {
        header_line(&tree.comparison, self.pr_header.as_ref()).bold().dimmed().to_string()
    }

    fn node(&self, out: &mut String, n: &TreeNode, prefix: &str, last: bool) {
        let conn = if last { "└──" } else { "├──" };
        let mark_str = mark(n, self.marks);
        let mark_render = match mark_color(&n.status) {
            Some(c) => mark_str.color(c).to_string(),
            None => mark_str.to_string(),
        };
        let metric = if n.kind == NodeKind::Directory {
            format!(
                " ({} files, {} {})",
                n.rollup.files_changed,
                add_str(n.rollup.churn.added),
                del_str(n.rollup.churn.deleted)
            )
        } else {
            format!(" {} {}", add_str(n.churn.added), del_str(n.churn.deleted))
        };
        let name = display_name(n);
        let abs = self.root.join(&n.path);
        let style = self.ls_colors.style_for_path(&abs).cloned().unwrap_or_default();
        let name_render = crate::style_name(&name, &style);
        out.push_str(&format!("{prefix}{conn} {mark_render} {name_render}{metric}\n"));
        let next = format!("{}{}", prefix, if last { "    " } else { "│   " });
        for (idx, c) in n.children.iter().enumerate() {
            self.node(out, c, &next, idx + 1 == n.children.len());
        }
    }
}

fn header_line(mode: &ComparisonMode, pr_header: Option<&PrHeaderContext>) -> String {
    match mode {
        ComparisonMode::Staged => "Staged changes".to_string(),
        ComparisonMode::Unstaged => "Unstaged changes".to_string(),
        ComparisonMode::Uncommitted => "Uncommitted changes (staged + unstaged)".to_string(),
        ComparisonMode::Range { range } => format!("Range: {range}"),
        ComparisonMode::Against { reference } => format!("Against: {reference}...working tree"),
        ComparisonMode::Pr { merge_base, committed } => {
            let endpoint = if *committed { "committed" } else { "working tree" };
            if let Some(ctx) = pr_header {
                if ctx.on_base {
                    format!(
                        "PR: {}...{} · on base branch (uncommitted only)",
                        ctx.base_ref, ctx.head_label
                    )
                } else {
                    format!("PR: {}...{} · {endpoint}", ctx.base_ref, ctx.head_label)
                }
            } else {
                format!("PR: {}...HEAD · {endpoint}", short_sha(merge_base))
            }
        }
    }
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

impl ChangeKind {
    fn label(self) -> &'static str {
        match self {
            ChangeKind::Added => "added",
            ChangeKind::Modified => "modified",
            ChangeKind::Deleted => "deleted",
            ChangeKind::Renamed => "renamed",
            ChangeKind::Copied => "copied",
            ChangeKind::Typechanged => "typechanged",
            ChangeKind::Conflicted => "conflicted",
            ChangeKind::Unreadable => "unreadable",
        }
    }

    fn color(self) -> colored::Color {
        use colored::Color;
        match self {
            ChangeKind::Added => Color::Green,
            ChangeKind::Modified => Color::Yellow,
            ChangeKind::Deleted => Color::Red,
            ChangeKind::Renamed => Color::Blue,
            ChangeKind::Copied => Color::BrightBlue,
            ChangeKind::Typechanged => Color::Cyan,
            ChangeKind::Conflicted => Color::BrightRed,
            ChangeKind::Unreadable => Color::BrightYellow,
        }
    }
}

const CHANGE_KIND_ORDER: [ChangeKind; 8] = [
    ChangeKind::Added,
    ChangeKind::Modified,
    ChangeKind::Deleted,
    ChangeKind::Renamed,
    ChangeKind::Copied,
    ChangeKind::Typechanged,
    ChangeKind::Conflicted,
    ChangeKind::Unreadable,
];

fn file_word(n: usize) -> &'static str {
    if n == 1 {
        "file"
    } else {
        "files"
    }
}

fn change_kind_tally(root: &TreeNode) -> BTreeMap<ChangeKind, usize> {
    fn walk(node: &TreeNode, out: &mut BTreeMap<ChangeKind, usize>) {
        if node.kind == NodeKind::File && node.status != ChangeStatus::Clean {
            if let Some(kind) = node.change_kind {
                *out.entry(kind).or_default() += 1;
            }
        }
        for child in &node.children {
            walk(child, out);
        }
    }

    let mut out = BTreeMap::new();
    walk(root, &mut out);
    out
}

fn kind_count_phrase(kind: ChangeKind, count: usize) -> String {
    format!("{count} {}", kind.label()).color(kind.color()).to_string()
}

fn file_count_phrase(tree: &ChangeTree) -> String {
    let tally = change_kind_tally(&tree.root);
    let nonzero: Vec<(ChangeKind, usize)> = CHANGE_KIND_ORDER
        .iter()
        .filter_map(|kind| tally.get(kind).copied().map(|count| (*kind, count)))
        .filter(|(_, count)| *count > 0)
        .collect();
    let total = tree.summary.files_changed;

    if nonzero.len() == 1 && nonzero[0].1 == total {
        let (kind, _) = nonzero[0];
        return format!("{total} {} {}", file_word(total), kind.label().color(kind.color()));
    }

    if nonzero.len() > 1 {
        let parts = nonzero
            .into_iter()
            .map(|(kind, count)| kind_count_phrase(kind, count))
            .collect::<Vec<_>>()
            .join(" · ");
        return format!("{total} {} changed ({parts})", file_word(total));
    }

    format!("{total} {} changed", file_word(total))
}

fn mark_color(s: &ChangeStatus) -> Option<colored::Color> {
    use colored::Color;
    match s {
        ChangeStatus::Staged => Some(Color::Green),
        ChangeStatus::Unstaged => Some(Color::Yellow),
        ChangeStatus::Both => Some(Color::Cyan),
        ChangeStatus::Untracked => Some(Color::Magenta),
        ChangeStatus::Renamed => Some(Color::Blue),
        ChangeStatus::Copied => Some(Color::BrightBlue),
        ChangeStatus::Deleted => Some(Color::Red),
        ChangeStatus::Typechanged => Some(Color::Cyan),
        ChangeStatus::Conflicted => Some(Color::BrightRed),
        ChangeStatus::Unreadable => Some(Color::BrightYellow),
        ChangeStatus::Ignored => Some(Color::BrightBlack),
        ChangeStatus::Clean => None,
    }
}
fn add_str(n: usize) -> String {
    format!("+{n}").green().to_string()
}
fn del_str(n: usize) -> String {
    format!("−{n}").red().to_string()
}
fn mark(n: &TreeNode, s: MarkScheme) -> &'static str {
    match s {
        MarkScheme::Symbol => match n.status {
            ChangeStatus::Staged => "●",
            ChangeStatus::Unstaged => "○",
            ChangeStatus::Both => "◐",
            ChangeStatus::Untracked => "?",
            ChangeStatus::Renamed => "↻",
            ChangeStatus::Copied => "⧉",
            ChangeStatus::Deleted => "×",
            ChangeStatus::Typechanged => "◆",
            ChangeStatus::Conflicted => "‼",
            ChangeStatus::Unreadable => "⚠",
            ChangeStatus::Ignored => "!",
            ChangeStatus::Clean => " ",
        },
        MarkScheme::Letter => match n.status {
            ChangeStatus::Staged => "S",
            ChangeStatus::Unstaged => "M",
            ChangeStatus::Both => "B",
            ChangeStatus::Untracked => "?",
            ChangeStatus::Renamed => "R",
            ChangeStatus::Copied => "C",
            ChangeStatus::Deleted => "D",
            ChangeStatus::Typechanged => "T",
            ChangeStatus::Conflicted => "U",
            ChangeStatus::Unreadable => "E",
            ChangeStatus::Ignored => "I",
            ChangeStatus::Clean => " ",
        },
        MarkScheme::Xy => match n.status {
            ChangeStatus::Staged => "M ",
            ChangeStatus::Unstaged => " M",
            ChangeStatus::Both => "MM",
            ChangeStatus::Untracked => "??",
            ChangeStatus::Renamed => "R ",
            ChangeStatus::Copied => "C ",
            ChangeStatus::Deleted => "D ",
            ChangeStatus::Typechanged => "T ",
            ChangeStatus::Conflicted => "UU",
            ChangeStatus::Unreadable => "E?",
            ChangeStatus::Ignored => "!!",
            ChangeStatus::Clean => "  ",
        },
    }
}
fn display_name(n: &TreeNode) -> String {
    if matches!(n.status, ChangeStatus::Renamed | ChangeStatus::Copied) {
        if let Some(old_path) = &n.old_path {
            let old = Path::new(old_path);
            let new = Path::new(&n.path);
            let same_parent =
                old.parent().unwrap_or(Path::new("")) == new.parent().unwrap_or(Path::new(""));
            let old_display = if same_parent {
                old.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| old_path.clone())
            } else {
                old_path.clone()
            };
            let new_display = if same_parent { n.name.clone() } else { n.path.clone() };
            let arrow = if n.status == ChangeStatus::Renamed { " -> " } else { " => " };
            return format!("{old_display}{arrow}{new_display}");
        }
    }
    n.name.clone()
}

#[derive(Debug, Clone)]
struct FileChange {
    path: PathBuf,
    old_path: Option<PathBuf>,
    status: ChangeStatus,
    change_kind: Option<ChangeKind>,
    churn: Churn,
}

/// The resolved base for a `--pr` comparison.
#[derive(Debug)]
pub struct PrBase {
    /// The base branch's short name (for messages), e.g. "main".
    pub base_name: String,
    /// The ref actually used, e.g. "origin/main" or an explicit "main".
    pub base_ref: String,
    /// The merge-base commit SHA between the base and HEAD.
    pub merge_base: String,
    /// True when the merge-base equals HEAD (no divergence from base).
    pub on_base: bool,
}

/// Reads the default branch name from `refs/remotes/origin/HEAD`, if present.
fn origin_default_branch(repo: &Repository) -> Option<String> {
    let r = repo.find_reference("refs/remotes/origin/HEAD").ok()?;
    let target = r.symbolic_target()?; // e.g. "refs/remotes/origin/main"
    target.strip_prefix("refs/remotes/origin/").map(|s| s.to_string())
}

/// Resolves the base branch for `--pr` and computes the merge-base with HEAD.
///
/// Base candidates (auto-detect): the `origin/HEAD` default branch, then `main`,
/// then `master`; auto-detected candidates prefer their remote-tracking ref
/// (`origin/<name>`) over the local branch. An explicit `base_override` is
/// resolved exactly first. Errors if no candidate resolves or there is no common
/// history.
pub fn resolve_pr_base(start: &Path, base_override: Option<&str>) -> anyhow::Result<PrBase> {
    let repo = Repository::discover(start)
        .map_err(|_| anyhow::anyhow!("difftree: --pr requires a git repository"))?;

    let mut candidates: Vec<String> = Vec::new();
    if let Some(o) = base_override {
        candidates.push(o.to_string());
    } else {
        if let Some(def) = origin_default_branch(&repo) {
            candidates.push(def);
        }
        candidates.push("main".to_string());
        candidates.push("master".to_string());
    }
    let mut seen = BTreeSet::new();
    candidates.retain(|c| seen.insert(c.clone()));

    let mut resolved: Option<(String, String, git2::Oid)> = None;
    if let Some(name) = base_override {
        let mut refs = vec![name.to_string()];
        if !name.contains('/') && !name.starts_with("refs/") {
            refs.push(format!("origin/{name}"));
        }
        for cand_ref in refs {
            if let Ok(obj) = repo.revparse_single(&cand_ref) {
                if let Ok(commit) = obj.peel_to_commit() {
                    resolved = Some((name.to_string(), cand_ref, commit.id()));
                    break;
                }
            }
        }
    } else {
        for name in &candidates {
            for cand_ref in [format!("origin/{name}"), name.clone()] {
                if let Ok(obj) = repo.revparse_single(&cand_ref) {
                    if let Ok(commit) = obj.peel_to_commit() {
                        resolved = Some((name.clone(), cand_ref, commit.id()));
                        break;
                    }
                }
            }
            if resolved.is_some() {
                break;
            }
        }
    }
    let (base_name, base_ref, base_oid) = resolved.ok_or_else(|| {
        anyhow::anyhow!(
            "difftree: could not resolve base branch (tried: {}); pass one with --pr=<ref> or --pr-base <ref>",
            candidates.join(", ")
        )
    })?;

    let head_oid = repo
        .head()
        .and_then(|h| h.peel_to_commit())
        .map_err(|e| anyhow::anyhow!("difftree: cannot read HEAD: {e}"))?
        .id();
    let mb = repo.merge_base(base_oid, head_oid).map_err(|_| {
        anyhow::anyhow!("difftree: no common history between HEAD and base '{base_name}'")
    })?;

    Ok(PrBase { base_name, base_ref, merge_base: mb.to_string(), on_base: mb == head_oid })
}

pub fn resolve_head_label(start: &Path) -> anyhow::Result<String> {
    let repo = Repository::discover(start)
        .map_err(|_| anyhow::anyhow!("difftree: --pr requires a git repository"))?;
    let head = repo.head()?;
    if head.is_branch() {
        if let Some(name) = head.shorthand() {
            return Ok(name.to_string());
        }
    }
    if let Some(oid) = head.target() {
        return Ok(short_sha(&oid.to_string()));
    }
    let oid = head.peel_to_commit()?.id();
    Ok(short_sha(&oid.to_string()))
}

pub fn collect_changes(
    start: &Path,
    mode: ComparisonMode,
    include_untracked: bool,
) -> anyhow::Result<Option<ChangeTree>> {
    let Ok(repo) = Repository::discover(start) else {
        return Ok(None);
    };
    let workdir =
        repo.workdir().ok_or_else(|| anyhow::anyhow!("bare repositories are not supported"))?;
    let scope = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let scope_rel = scope_relative_path(&scope, workdir);
    let effective = mode.clone();
    let mut files = diff_files(&repo, &effective)?;
    if includes_worktree_statuses(&mode) {
        add_conflicted(&repo, &mut files)?;
    }
    if include_untracked {
        add_untracked(&repo, &mut files)?;
    }
    let committed_head_tree = head_tree_for_committed_pr(&repo, &mode)?;
    files = files
        .into_iter()
        .filter_map(|f| {
            let mut f = scoped_file_change(f, &scope_rel)?;
            normalize_committed_scoped_rename(&mut f, &scope_rel, committed_head_tree.as_ref());
            Some(f)
        })
        .collect();
    let root_name = if scope_rel.as_os_str().is_empty() {
        workdir.file_name().and_then(|s| s.to_str()).unwrap_or(".").to_string()
    } else {
        scope_rel.display().to_string()
    };
    let (dirset, fmap) = files_to_maps(files);
    Ok(Some(build_tree(root_name, mode, View::BlastRadius, dirset, fmap, None)))
}

pub fn collect_default_with_fallback(start: &Path) -> anyhow::Result<Option<ChangeTree>> {
    let mut staged = collect_changes(start, ComparisonMode::Staged, true)?;
    if staged.as_ref().is_some_and(|t| t.summary.files_changed > 0) {
        return Ok(staged);
    }
    let mut unstaged = collect_changes(start, ComparisonMode::Unstaged, true)?;
    if unstaged.as_ref().is_some_and(|t| t.summary.files_changed > 0) {
        if let Some(t) = &mut unstaged {
            t.fallback = Some("No staged changes — showing unstaged changes".to_string());
        }
        staged = unstaged;
    }
    Ok(staged)
}

/// Builds the raw git diff for a comparison mode (without per-file line stats,
/// which are expensive to compute). Callers that only need existence can check
/// `diff.deltas().len()`; `diff_files` adds churn on top.
fn build_diff<'r>(repo: &'r Repository, mode: &ComparisonMode) -> anyhow::Result<git2::Diff<'r>> {
    build_diff_for_paths(repo, mode, None)
}

fn build_diff_for_paths<'r>(
    repo: &'r Repository,
    mode: &ComparisonMode,
    pathspecs: Option<&BTreeSet<PathBuf>>,
) -> anyhow::Result<git2::Diff<'r>> {
    let mut opts = DiffOptions::new();
    opts.include_untracked(false)
        .recurse_untracked_dirs(true)
        .include_unmodified(true)
        .include_typechange(true)
        .include_unreadable(true);
    if let Some(pathspecs) = pathspecs {
        for path in pathspecs {
            opts.pathspec(path.as_path());
        }
    }
    let mut diff = match mode {
        ComparisonMode::Staged => {
            let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            let idx = repo.index()?;
            repo.diff_tree_to_index(head.as_ref(), Some(&idx), Some(&mut opts))?
        }
        ComparisonMode::Unstaged => repo.diff_index_to_workdir(None, Some(&mut opts))?,
        ComparisonMode::Uncommitted => repo.diff_tree_to_workdir_with_index(
            repo.head().ok().and_then(|h| h.peel_to_tree().ok()).as_ref(),
            Some(&mut opts),
        )?,
        ComparisonMode::Against { reference } => {
            let obj = repo.revparse_single(reference)?;
            let tree = obj.peel_to_tree()?;
            repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts))?
        }
        ComparisonMode::Range { range } => {
            let (a, b) =
                range.split_once("..").ok_or_else(|| anyhow::anyhow!("range must use A..B"))?;
            let ta = repo.revparse_single(a)?.peel_to_tree()?;
            let tb = repo.revparse_single(b)?.peel_to_tree()?;
            repo.diff_tree_to_tree(Some(&ta), Some(&tb), Some(&mut opts))?
        }
        ComparisonMode::Pr { merge_base, committed } => {
            let mb_tree = repo.revparse_single(merge_base)?.peel_to_tree()?;
            if *committed {
                let head_tree = repo.head()?.peel_to_tree()?;
                repo.diff_tree_to_tree(Some(&mb_tree), Some(&head_tree), Some(&mut opts))?
            } else {
                repo.diff_tree_to_workdir_with_index(Some(&mb_tree), Some(&mut opts))?
            }
        }
    };
    let mut find = DiffFindOptions::new();
    find.renames(true);
    find.copies(true);
    find.copies_from_unmodified(true);
    find.remove_unmodified(true);
    diff.find_similar(Some(&mut find))?;
    Ok(diff)
}

fn diff_error_is_locked(err: &anyhow::Error) -> bool {
    err.downcast_ref::<git2::Error>().is_some_and(|e| e.code() == git2::ErrorCode::Locked)
}

fn mode_uses_workdir_diff(mode: &ComparisonMode) -> bool {
    matches!(
        mode,
        ComparisonMode::Unstaged
            | ComparisonMode::Uncommitted
            | ComparisonMode::Against { .. }
            | ComparisonMode::Pr { committed: false, .. }
    )
}

fn status_for_delta(delta: git2::Delta, mode: &ComparisonMode) -> ChangeStatus {
    match delta {
        git2::Delta::Deleted => ChangeStatus::Deleted,
        git2::Delta::Renamed => ChangeStatus::Renamed,
        git2::Delta::Copied => ChangeStatus::Copied,
        git2::Delta::Typechange => ChangeStatus::Typechanged,
        git2::Delta::Conflicted => ChangeStatus::Conflicted,
        git2::Delta::Unreadable => ChangeStatus::Unreadable,
        _ => match mode {
            ComparisonMode::Unstaged => ChangeStatus::Unstaged,
            _ => ChangeStatus::Staged,
        },
    }
}

fn change_kind_for_delta(delta: git2::Delta) -> ChangeKind {
    match delta {
        git2::Delta::Added | git2::Delta::Untracked => ChangeKind::Added,
        git2::Delta::Deleted => ChangeKind::Deleted,
        git2::Delta::Renamed => ChangeKind::Renamed,
        git2::Delta::Copied => ChangeKind::Copied,
        git2::Delta::Typechange => ChangeKind::Typechanged,
        git2::Delta::Conflicted => ChangeKind::Conflicted,
        git2::Delta::Unreadable => ChangeKind::Unreadable,
        _ => ChangeKind::Modified,
    }
}

fn pr_worktree_status_overrides(
    repo: &Repository,
) -> anyhow::Result<BTreeMap<PathBuf, ChangeStatus>> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(false).recurse_untracked_dirs(true).include_unreadable(true);
    let statuses = match repo.statuses(Some(&mut opts)) {
        Ok(statuses) => statuses,
        Err(err) if err.code() == git2::ErrorCode::Locked => return Ok(BTreeMap::new()),
        Err(err) => return Err(err.into()),
    };
    let mut out = BTreeMap::new();
    for entry in statuses.iter() {
        let Some(path) = entry.path() else { continue };
        let status = entry.status();
        let change_status = if status.is_wt_modified()
            || status.is_wt_deleted()
            || status.is_wt_typechange()
            || status.is_wt_renamed()
        {
            Some(ChangeStatus::Unstaged)
        } else if status.is_index_new()
            || status.is_index_modified()
            || status.is_index_deleted()
            || status.is_index_typechange()
            || status.is_index_renamed()
        {
            Some(ChangeStatus::Staged)
        } else {
            None
        };
        if let Some(change_status) = change_status {
            out.insert(PathBuf::from(path), change_status);
        }
    }
    Ok(out)
}

fn add_index_paths(repo: &Repository, paths: &mut BTreeSet<PathBuf>) -> anyhow::Result<()> {
    for entry in repo.index()?.iter() {
        if let Ok(path) = std::str::from_utf8(&entry.path) {
            paths.insert(PathBuf::from(path));
        }
    }
    Ok(())
}

fn add_tree_paths(tree: &git2::Tree<'_>, paths: &mut BTreeSet<PathBuf>) -> anyhow::Result<()> {
    tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            if let Some(name) = entry.name() {
                paths.insert(Path::new(root).join(name));
            }
        }
        git2::TreeWalkResult::Ok
    })?;
    Ok(())
}

fn tracked_paths_for_workdir_mode(
    repo: &Repository,
    mode: &ComparisonMode,
) -> anyhow::Result<BTreeSet<PathBuf>> {
    let mut paths = BTreeSet::new();
    add_index_paths(repo, &mut paths)?;
    match mode {
        ComparisonMode::Uncommitted => {
            if let Ok(tree) = repo.head().and_then(|h| h.peel_to_tree()) {
                add_tree_paths(&tree, &mut paths)?;
            }
        }
        ComparisonMode::Against { reference } => {
            let tree = repo.revparse_single(reference)?.peel_to_tree()?;
            add_tree_paths(&tree, &mut paths)?;
        }
        ComparisonMode::Pr { merge_base, committed: false } => {
            let tree = repo.revparse_single(merge_base)?.peel_to_tree()?;
            add_tree_paths(&tree, &mut paths)?;
        }
        _ => {}
    }
    Ok(paths)
}

fn unreadable_regular_paths(repo: &Repository, paths: &BTreeSet<PathBuf>) -> BTreeSet<PathBuf> {
    let Some(workdir) = repo.workdir() else {
        return BTreeSet::new();
    };
    paths
        .iter()
        .filter_map(|path| {
            let abs = workdir.join(path);
            let meta = std::fs::symlink_metadata(&abs).ok()?;
            if !meta.file_type().is_file() {
                return None;
            }
            match std::fs::File::open(&abs) {
                Ok(_) => None,
                Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                    Some(path.clone())
                }
                Err(_) => None,
            }
        })
        .collect()
}

fn file_changes_from_diff(
    repo: &Repository,
    diff: &git2::Diff<'_>,
    mode: &ComparisonMode,
) -> anyhow::Result<Vec<FileChange>> {
    let pr_worktree_statuses = if matches!(mode, ComparisonMode::Pr { committed: false, .. }) {
        pr_worktree_status_overrides(repo)?
    } else {
        BTreeMap::new()
    };
    let mut out = Vec::new();
    for idx in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(idx) else { continue };
        let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) else {
            continue;
        };
        let mut status = status_for_delta(delta.status(), mode);
        let change_kind = Some(change_kind_for_delta(delta.status()));
        if status == ChangeStatus::Staged {
            if let Some(worktree_status) = pr_worktree_statuses.get(path) {
                status = worktree_status.clone();
            }
        }
        let old_path = if matches!(status, ChangeStatus::Renamed | ChangeStatus::Copied) {
            delta.old_file().path().map(|p| p.to_path_buf())
        } else {
            None
        };
        // Per-file line stats; binary or patch-less deltas have no textual churn.
        let churn = if status == ChangeStatus::Unreadable {
            Churn::default()
        } else {
            match git2::Patch::from_diff(diff, idx)? {
                Some(patch) => {
                    let (_context, added, deleted) = patch.line_stats()?;
                    Churn { added, deleted }
                }
                None => Churn::default(),
            }
        };
        out.push(FileChange { path: path.to_path_buf(), old_path, status, change_kind, churn });
    }
    Ok(out)
}

fn diff_files_with_unreadable_fallback(
    repo: &Repository,
    mode: &ComparisonMode,
) -> anyhow::Result<Vec<FileChange>> {
    let tracked_paths = tracked_paths_for_workdir_mode(repo, mode)?;
    let unreadable_paths = unreadable_regular_paths(repo, &tracked_paths);
    if unreadable_paths.is_empty() {
        return build_diff(repo, mode).and_then(|diff| file_changes_from_diff(repo, &diff, mode));
    }

    let readable_paths: BTreeSet<PathBuf> =
        tracked_paths.difference(&unreadable_paths).cloned().collect();
    let mut out = if readable_paths.is_empty() {
        Vec::new()
    } else {
        let diff = build_diff_for_paths(repo, mode, Some(&readable_paths))?;
        file_changes_from_diff(repo, &diff, mode)?
    };

    let existing: BTreeSet<PathBuf> = out.iter().map(|f| f.path.clone()).collect();
    for path in unreadable_paths {
        if !existing.contains(&path) {
            out.push(FileChange {
                path,
                old_path: None,
                status: ChangeStatus::Unreadable,
                change_kind: Some(ChangeKind::Unreadable),
                churn: Churn::default(),
            });
        }
    }
    Ok(out)
}

fn diff_files(repo: &Repository, mode: &ComparisonMode) -> anyhow::Result<Vec<FileChange>> {
    match build_diff(repo, mode) {
        Ok(diff) => file_changes_from_diff(repo, &diff, mode),
        Err(err) if mode_uses_workdir_diff(mode) && diff_error_is_locked(&err) => {
            diff_files_with_unreadable_fallback(repo, mode)
        }
        Err(err) => Err(err),
    }
}
fn add_conflicted(repo: &Repository, files: &mut Vec<FileChange>) -> anyhow::Result<()> {
    let existing: BTreeSet<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(false).recurse_untracked_dirs(true).include_unreadable(true);
    let statuses = match repo.statuses(Some(&mut opts)) {
        Ok(statuses) => statuses,
        Err(err) if err.code() == git2::ErrorCode::Locked => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    for e in statuses.iter() {
        if e.status().is_conflicted() {
            if let Some(p) = e.path() {
                let path = PathBuf::from(p);
                if !existing.contains(&path) {
                    files.push(FileChange {
                        path,
                        old_path: None,
                        status: ChangeStatus::Conflicted,
                        change_kind: Some(ChangeKind::Conflicted),
                        churn: Churn::default(),
                    });
                }
            }
        }
    }
    Ok(())
}
fn count_text_lines_streaming(abs: &Path) -> std::io::Result<Option<usize>> {
    let file = std::fs::File::open(abs)?;
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut lines = 0;
    loop {
        buf.clear();
        let read = reader.read_until(b'\n', &mut buf)?;
        if read == 0 {
            break;
        }
        if buf.contains(&0) || std::str::from_utf8(&buf).is_err() {
            return Ok(None);
        }
        lines += 1;
    }
    Ok(Some(lines))
}

fn status_kind_and_churn_for_untracked(abs: &Path) -> (ChangeStatus, Option<ChangeKind>, Churn) {
    match count_text_lines_streaming(abs) {
        Ok(Some(added)) => {
            (ChangeStatus::Untracked, Some(ChangeKind::Added), Churn { added, deleted: 0 })
        }
        Ok(None) => (ChangeStatus::Untracked, Some(ChangeKind::Added), Churn::default()),
        Err(_) => (ChangeStatus::Unreadable, Some(ChangeKind::Unreadable), Churn::default()),
    }
}

fn untracked_paths_from_workdir_walk(repo: &Repository) -> anyhow::Result<BTreeSet<PathBuf>> {
    let Some(workdir) = repo.workdir() else {
        return Ok(BTreeSet::new());
    };
    let mut tracked = BTreeSet::new();
    add_index_paths(repo, &mut tracked)?;
    let mut out = BTreeSet::new();
    let mut builder = ignore::WalkBuilder::new(workdir);
    builder.hidden(false).git_ignore(true).filter_entry(|entry| entry.file_name() != ".git");
    for result in builder.build() {
        let Ok(entry) = result else { continue };
        if entry.depth() == 0 || entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(workdir) else { continue };
        let rel = rel.to_path_buf();
        if tracked.contains(&rel) || repo.status_should_ignore(&rel).unwrap_or(false) {
            continue;
        }
        out.insert(rel);
    }
    Ok(out)
}

fn worktree_only_paths_from_workdir_walk(repo: &Repository) -> anyhow::Result<BTreeSet<PathBuf>> {
    let Some(workdir) = repo.workdir() else {
        return Ok(BTreeSet::new());
    };
    let mut tracked = BTreeSet::new();
    add_index_paths(repo, &mut tracked)?;
    let mut out = BTreeSet::new();
    let mut builder = ignore::WalkBuilder::new(workdir);
    builder.hidden(false).git_ignore(false).filter_entry(|entry| entry.file_name() != ".git");
    for result in builder.build() {
        let Ok(entry) = result else { continue };
        if entry.depth() == 0 || entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(workdir) else { continue };
        let rel = rel.to_path_buf();
        if !tracked.contains(&rel) {
            out.insert(rel);
        }
    }
    Ok(out)
}

fn worktree_only_paths(repo: &Repository) -> anyhow::Result<BTreeSet<PathBuf>> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(true)
        .recurse_ignored_dirs(true)
        .include_unreadable(true);
    let statuses = match repo.statuses(Some(&mut opts)) {
        Ok(statuses) => statuses,
        Err(err) if err.code() == git2::ErrorCode::Locked => {
            return worktree_only_paths_from_workdir_walk(repo);
        }
        Err(err) => return Err(err.into()),
    };
    Ok(statuses
        .iter()
        .filter_map(|e| {
            if e.status().is_wt_new() || e.status().is_ignored() {
                e.path().map(PathBuf::from)
            } else {
                None
            }
        })
        .collect())
}

fn add_untracked_from_workdir_walk(
    repo: &Repository,
    files: &mut Vec<FileChange>,
) -> anyhow::Result<()> {
    let Some(workdir) = repo.workdir() else {
        return Ok(());
    };
    let untracked = untracked_paths_from_workdir_walk(repo)?;
    let mut existing: BTreeSet<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
    for rel in untracked {
        if existing.contains(&rel) {
            continue;
        }
        let (status, change_kind, churn) = status_kind_and_churn_for_untracked(&workdir.join(&rel));
        files.push(FileChange { path: rel.clone(), old_path: None, status, change_kind, churn });
        existing.insert(rel);
    }
    Ok(())
}

fn add_untracked(repo: &Repository, files: &mut Vec<FileChange>) -> anyhow::Result<()> {
    let workdir = repo.workdir();
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true).include_unreadable(true);
    let statuses = match repo.statuses(Some(&mut opts)) {
        Ok(statuses) => statuses,
        Err(err) if err.code() == git2::ErrorCode::Locked => {
            return add_untracked_from_workdir_walk(repo, files);
        }
        Err(err) => return Err(err.into()),
    };
    for e in statuses.iter() {
        if e.status().is_wt_new() {
            if let Some(p) = e.path() {
                let (status, change_kind, churn) =
                    workdir.map(|w| status_kind_and_churn_for_untracked(&w.join(p))).unwrap_or((
                        ChangeStatus::Unreadable,
                        Some(ChangeKind::Unreadable),
                        Churn::default(),
                    ));
                files.push(FileChange {
                    path: PathBuf::from(p),
                    old_path: None,
                    status,
                    change_kind,
                    churn,
                });
            }
        }
    }
    Ok(())
}
fn files_to_maps(files: Vec<FileChange>) -> (BTreeSet<PathBuf>, BTreeMap<PathBuf, FileChange>) {
    let mut dirset = BTreeSet::new();
    let mut fmap = BTreeMap::new();
    for f in files {
        add_ancestor_dirs(&f.path, &mut dirset);
        fmap.insert(f.path.clone(), f);
    }
    (dirset, fmap)
}

fn add_ancestor_dirs(path: &Path, dirset: &mut BTreeSet<PathBuf>) {
    for a in path.ancestors().skip(1) {
        if !a.as_os_str().is_empty() {
            dirset.insert(a.to_path_buf());
        }
    }
}

fn repo_path_segments(path: &Path) -> Vec<String> {
    path.to_string_lossy()
        .replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .map(str::to_string)
        .collect()
}

fn path_from_segments(segments: &[String]) -> PathBuf {
    let mut out = PathBuf::new();
    for segment in segments {
        out.push(segment);
    }
    out
}

fn path_prefix_segments(path: &Path) -> Vec<String> {
    repo_path_segments(path).into_iter().filter(|segment| segment != "?").collect()
}

fn segments_start_with(path: &[String], prefix: &[String]) -> bool {
    path.len() >= prefix.len()
        && path
            .iter()
            .zip(prefix)
            .all(|(path, prefix)| path == prefix || path.eq_ignore_ascii_case(prefix))
}

fn scope_relative_path(scope: &Path, workdir: &Path) -> PathBuf {
    if let Ok(rel) = scope.strip_prefix(workdir) {
        return rel.to_path_buf();
    }

    let scope_segments = path_prefix_segments(scope);
    let workdir_segments = path_prefix_segments(workdir);
    if segments_start_with(&scope_segments, &workdir_segments) {
        path_from_segments(&scope_segments[workdir_segments.len()..])
    } else {
        PathBuf::new()
    }
}

fn strip_repo_prefix(path: &Path, prefix: &Path) -> Option<PathBuf> {
    if prefix.as_os_str().is_empty() {
        return Some(path.to_path_buf());
    }

    let path_segments = repo_path_segments(path);
    let prefix_segments = repo_path_segments(prefix);
    if prefix_segments.is_empty() {
        return Some(path.to_path_buf());
    }
    if path_segments.len() < prefix_segments.len()
        || path_segments.iter().zip(&prefix_segments).any(|(path, prefix)| path != prefix)
    {
        return None;
    }

    Some(path_from_segments(&path_segments[prefix_segments.len()..]))
}

fn head_tree_for_committed_pr<'repo>(
    repo: &'repo Repository,
    mode: &ComparisonMode,
) -> anyhow::Result<Option<git2::Tree<'repo>>> {
    if matches!(mode, ComparisonMode::Pr { committed: true, .. }) {
        Ok(Some(repo.head()?.peel_to_tree()?))
    } else {
        Ok(None)
    }
}

fn tree_contains_repo_path(tree: &git2::Tree<'_>, path: &Path) -> bool {
    let repo_path = repo_path_segments(path).join("/");
    !repo_path.is_empty() && tree.get_path(Path::new(&repo_path)).is_ok()
}

fn normalize_committed_scoped_rename(
    f: &mut FileChange,
    scope_rel: &Path,
    head_tree: Option<&git2::Tree<'_>>,
) {
    if f.status != ChangeStatus::Renamed {
        return;
    }
    let Some(head_tree) = head_tree else { return };
    let repo_path =
        if scope_rel.as_os_str().is_empty() { f.path.clone() } else { scope_rel.join(&f.path) };
    if !tree_contains_repo_path(head_tree, &repo_path) {
        f.status = ChangeStatus::Deleted;
        f.change_kind = Some(ChangeKind::Deleted);
        f.old_path = None;
    }
}

fn scoped_file_change(mut f: FileChange, scope_rel: &Path) -> Option<FileChange> {
    if scope_rel.as_os_str().is_empty() {
        return Some(f);
    }

    if let Some(path_rel) = strip_repo_prefix(&f.path, scope_rel) {
        f.path = path_rel;
        if let Some(old_path) = &mut f.old_path {
            if let Some(old_rel) = strip_repo_prefix(old_path, scope_rel) {
                *old_path = old_rel;
            }
        }
        return Some(f);
    }

    if f.status == ChangeStatus::Renamed {
        if let Some(old_path) = f.old_path.take() {
            if let Some(old_rel) = strip_repo_prefix(&old_path, scope_rel) {
                f.path = old_rel;
                f.status = ChangeStatus::Deleted;
                f.change_kind = Some(ChangeKind::Deleted);
                return Some(f);
            }
        }
    }

    None
}

fn path_matches_worktree_only(rel: &Path, worktree_only: &BTreeSet<PathBuf>) -> bool {
    worktree_only.contains(rel)
        || rel.ancestors().skip(1).any(|parent| worktree_only.contains(parent))
}

#[derive(Debug, Clone, Copy)]
pub struct WalkOpts {
    pub all: bool,
    pub gitignore: bool,
    pub level: Option<usize>,
    pub dirs_only: bool,
}

pub fn collect_all_files(
    start: &Path,
    mode: ComparisonMode,
    opts: WalkOpts,
) -> anyhow::Result<Option<ChangeTree>> {
    let Ok(repo) = Repository::discover(start) else {
        return Ok(None);
    };
    let workdir =
        repo.workdir().ok_or_else(|| anyhow::anyhow!("bare repositories are not supported"))?;
    let scope = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let scope_rel = scope_relative_path(&scope, workdir);
    let committed_head_tree = head_tree_for_committed_pr(&repo, &mode)?;

    // Build the git change map, re-keyed relative to the scope root.
    let mut changed = diff_files(&repo, &mode)?;
    let include_untracked = match &mode {
        ComparisonMode::Range { .. } => false,
        ComparisonMode::Pr { committed, .. } => !committed,
        _ => true,
    };
    if includes_worktree_statuses(&mode) {
        add_conflicted(&repo, &mut changed)?;
    }
    if include_untracked {
        add_untracked(&repo, &mut changed)?;
    }
    let mut change_map: BTreeMap<PathBuf, FileChange> = BTreeMap::new();
    for f in changed {
        if let Some(mut f) = scoped_file_change(f, &scope_rel) {
            normalize_committed_scoped_rename(&mut f, &scope_rel, committed_head_tree.as_ref());
            change_map.insert(f.path.clone(), f);
        }
    }

    // For Pr committed mode, build a set of worktree-only paths to exclude from
    // the filesystem walk (the change_map guard above only controls status labels,
    // not which entries the walker surfaces).
    let worktree_only_rel: BTreeSet<PathBuf> =
        if matches!(mode, ComparisonMode::Pr { committed: true, .. }) {
            worktree_only_paths(&repo)?
                .into_iter()
                .filter_map(|f| {
                    if scope_rel.as_os_str().is_empty() {
                        Some(f)
                    } else {
                        strip_repo_prefix(&f, &scope_rel)
                    }
                })
                .collect()
        } else {
            BTreeSet::new()
        };

    // Walk the filesystem; keys are paths relative to the scope root.
    let mut builder = ignore::WalkBuilder::new(start);
    builder.hidden(!opts.all).git_ignore(opts.gitignore);
    if let Some(level) = opts.level {
        builder.max_depth(Some(level));
    }
    let mut dirset: BTreeSet<PathBuf> = BTreeSet::new();
    let mut fmap: BTreeMap<PathBuf, FileChange> = BTreeMap::new();
    let mut visible_files: BTreeSet<PathBuf> = BTreeSet::new();
    for result in builder.build() {
        let Ok(entry) = result else { continue };
        if entry.depth() == 0 {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(start) else { continue };
        let rel = rel.to_path_buf();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if path_matches_worktree_only(&rel, &worktree_only_rel) {
                continue;
            }
            dirset.insert(rel);
        } else {
            if path_matches_worktree_only(&rel, &worktree_only_rel) {
                continue;
            }
            visible_files.insert(rel.clone());
            if opts.dirs_only {
                continue;
            }
            let fc = change_map.get(&rel).cloned().unwrap_or_else(|| FileChange {
                path: rel.clone(),
                old_path: None,
                status: ChangeStatus::Clean,
                change_kind: None,
                churn: Churn::default(),
            });
            fmap.insert(rel, fc);
        }
    }
    for (rel, fc) in change_map {
        visible_files.insert(rel.clone());
        if opts.dirs_only {
            continue;
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = fmap.entry(rel) {
            add_ancestor_dirs(entry.key(), &mut dirset);
            entry.insert(fc);
        }
    }
    if matches!(mode, ComparisonMode::Pr { committed: true, .. }) {
        dirset.retain(|dir| visible_files.iter().any(|file| file.starts_with(dir)));
    }

    let root_name = if scope_rel.as_os_str().is_empty() {
        workdir.file_name().and_then(|s| s.to_str()).unwrap_or(".").to_string()
    } else {
        scope_rel.display().to_string()
    };
    Ok(Some(build_tree(root_name, mode, View::AllFiles, dirset, fmap, None)))
}

fn includes_worktree_statuses(mode: &ComparisonMode) -> bool {
    !matches!(mode, ComparisonMode::Range { .. } | ComparisonMode::Pr { committed: true, .. })
}

fn has_changes(repo: &Repository, mode: &ComparisonMode) -> anyhow::Result<bool> {
    // Existence only — avoid the per-file patch/line-count work of diff_files.
    match build_diff(repo, mode) {
        Ok(diff) if diff.deltas().len() > 0 => return Ok(true),
        Ok(_) => {}
        Err(err) if mode_uses_workdir_diff(mode) && diff_error_is_locked(&err) => return Ok(true),
        Err(err) => return Err(err),
    }
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true).include_unreadable(true);
    match repo.statuses(Some(&mut opts)) {
        Ok(statuses) => Ok(statuses.iter().any(|e| e.status().is_wt_new())),
        Err(err) if err.code() == git2::ErrorCode::Locked => Ok(true),
        Err(err) => Err(err.into()),
    }
}

/// All-files view with the same staged -> unstaged auto-fallback as the bare
/// default: if there are no staged changes, overlay the unstaged comparison and
/// label it. The fallback decision is based on the real diff, independent of any
/// `-L`/`-d` view filters.
pub fn collect_all_files_default_with_fallback(
    start: &Path,
    opts: WalkOpts,
) -> anyhow::Result<Option<ChangeTree>> {
    let Ok(repo) = Repository::discover(start) else {
        return Ok(None);
    };
    if has_changes(&repo, &ComparisonMode::Staged)? {
        return collect_all_files(start, ComparisonMode::Staged, opts);
    }
    if has_changes(&repo, &ComparisonMode::Unstaged)? {
        let mut tree = collect_all_files(start, ComparisonMode::Unstaged, opts)?;
        if let Some(t) = &mut tree {
            t.fallback = Some("No staged changes — showing unstaged changes".to_string());
        }
        return Ok(tree);
    }
    collect_all_files(start, ComparisonMode::Staged, opts)
}

fn build_tree(
    root_name: String,
    mode: ComparisonMode,
    view: View,
    dirset: BTreeSet<PathBuf>,
    fmap: BTreeMap<PathBuf, FileChange>,
    fallback: Option<String>,
) -> ChangeTree {
    fn child_paths(
        parent: &Path,
        dirs: &BTreeSet<PathBuf>,
        files: &BTreeMap<PathBuf, FileChange>,
    ) -> Vec<PathBuf> {
        let mut s = BTreeSet::new();
        for d in dirs {
            if d.parent().unwrap_or(Path::new("")) == parent {
                s.insert(d.clone());
            }
        }
        for f in files.keys() {
            if f.parent().unwrap_or(Path::new("")) == parent {
                s.insert(f.clone());
            }
        }
        s.into_iter().collect()
    }
    fn mk(
        path: &Path,
        dirs: &BTreeSet<PathBuf>,
        files: &BTreeMap<PathBuf, FileChange>,
    ) -> TreeNode {
        if let Some(f) = files.get(path) {
            TreeNode {
                name: path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string()),
                path: path.display().to_string(),
                old_path: f.old_path.as_ref().map(|p| p.display().to_string()),
                kind: NodeKind::File,
                change_kind: f.change_kind,
                status: f.status.clone(),
                churn: f.churn.clone(),
                rollup: Rollup {
                    dirs_touched: 0,
                    files_changed: if f.status == ChangeStatus::Clean { 0 } else { 1 },
                    churn: f.churn.clone(),
                },
                children: vec![],
            }
        } else {
            let mut ch: Vec<_> =
                child_paths(path, dirs, files).iter().map(|p| mk(p, dirs, files)).collect();
            ch.sort_by(|a, b| a.name.cmp(&b.name));
            let mut r = Rollup::default();
            for c in &ch {
                if c.kind == NodeKind::Directory {
                    r.dirs_touched +=
                        c.rollup.dirs_touched + usize::from(c.rollup.files_changed > 0);
                }
                r.files_changed += c.rollup.files_changed;
                r.churn.added += c.rollup.churn.added;
                r.churn.deleted += c.rollup.churn.deleted;
            }
            TreeNode {
                name: path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| root_name_from_path(path)),
                path: path.display().to_string(),
                old_path: None,
                kind: NodeKind::Directory,
                change_kind: None,
                status: ChangeStatus::Clean,
                churn: Churn::default(),
                rollup: r,
                children: ch,
            }
        }
    }
    fn root_name_from_path(p: &Path) -> String {
        if p.as_os_str().is_empty() {
            ".".into()
        } else {
            p.display().to_string()
        }
    }
    let mut children: Vec<_> =
        child_paths(Path::new(""), &dirset, &fmap).iter().map(|p| mk(p, &dirset, &fmap)).collect();
    children.sort_by(|a, b| a.name.cmp(&b.name));
    let mut summary = Rollup::default();
    for c in &children {
        if c.kind == NodeKind::Directory {
            summary.dirs_touched += c.rollup.dirs_touched + usize::from(c.rollup.files_changed > 0);
        }
        summary.files_changed += c.rollup.files_changed;
        summary.churn.added += c.rollup.churn.added;
        summary.churn.deleted += c.rollup.churn.deleted;
    }
    let root = TreeNode {
        name: root_name,
        path: "".into(),
        old_path: None,
        kind: NodeKind::Directory,
        change_kind: None,
        status: ChangeStatus::Clean,
        churn: Churn::default(),
        rollup: summary.clone(),
        children,
    };
    ChangeTree {
        schema_version: SCHEMA_VERSION.into(),
        comparison: mode,
        view,
        root,
        summary,
        fallback,
    }
}

#[cfg(test)]
mod pr_tests {
    use super::*;
    use std::process::Command as Pcmd;

    fn git(dir: &Path, args: &[&str]) {
        let output = Pcmd::new("git").args(args).current_dir(dir).output().unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fn git_fails(dir: &Path, args: &[&str]) {
        let output = Pcmd::new("git").args(args).current_dir(dir).output().unwrap();
        assert!(
            !output.status.success(),
            "git {:?} unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fn git_out(dir: &Path, args: &[&str]) -> String {
        let o = Pcmd::new("git").args(args).current_dir(dir).output().unwrap();
        assert!(
            o.status.success(),
            "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8(o.stdout).unwrap().trim().to_string()
    }
    fn file_names(tree: &ChangeTree) -> Vec<String> {
        fn walk(n: &TreeNode, out: &mut Vec<String>) {
            if n.kind == NodeKind::File {
                out.push(n.name.clone());
            }
            for c in &n.children {
                walk(c, out);
            }
        }
        let mut v = Vec::new();
        walk(&tree.root, &mut v);
        v
    }

    fn dir_paths(tree: &ChangeTree) -> Vec<String> {
        fn walk(n: &TreeNode, out: &mut Vec<String>) {
            if n.kind == NodeKind::Directory {
                out.push(n.path.clone());
            }
            for c in &n.children {
                walk(c, out);
            }
        }
        let mut v = Vec::new();
        walk(&tree.root, &mut v);
        v
    }

    fn find_file_churn(tree: &ChangeTree, name: &str) -> Option<Churn> {
        fn walk(n: &TreeNode, name: &str) -> Option<Churn> {
            if n.kind == NodeKind::File && n.name == name {
                return Some(n.churn.clone());
            }
            for c in &n.children {
                if let Some(ch) = walk(c, name) {
                    return Some(ch);
                }
            }
            None
        }
        walk(&tree.root, name)
    }

    fn find_file_node<'a>(tree: &'a ChangeTree, name: &str) -> Option<&'a TreeNode> {
        fn walk<'a>(n: &'a TreeNode, name: &str) -> Option<&'a TreeNode> {
            if n.kind == NodeKind::File && n.name == name {
                return Some(n);
            }
            for c in &n.children {
                if let Some(found) = walk(c, name) {
                    return Some(found);
                }
            }
            None
        }
        walk(&tree.root, name)
    }

    #[test]
    fn change_kinds_distinguish_staged_added_and_modified() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("edited.txt"), "one\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        std::fs::write(p.join("edited.txt"), "one\ntwo\n").unwrap();
        std::fs::write(p.join("added.txt"), "new\n").unwrap();
        git(p, &["add", "."]);

        let tree = collect_changes(p, ComparisonMode::Staged, false).unwrap().unwrap();
        let added = find_file_node(&tree, "added.txt").expect("added.txt present");
        let edited = find_file_node(&tree, "edited.txt").expect("edited.txt present");

        assert_eq!(added.change_kind, Some(ChangeKind::Added));
        assert_eq!(edited.change_kind, Some(ChangeKind::Modified));
        assert_eq!(added.status, ChangeStatus::Staged);
        assert_eq!(edited.status, ChangeStatus::Staged);
    }

    #[test]
    fn untracked_file_kind_is_added() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("seed.txt"), "seed\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        std::fs::write(p.join("new.txt"), "new\n").unwrap();

        let tree = collect_changes(p, ComparisonMode::Staged, true).unwrap().unwrap();
        let node = find_file_node(&tree, "new.txt").expect("new.txt present");

        assert_eq!(node.status, ChangeStatus::Untracked);
        assert_eq!(node.change_kind, Some(ChangeKind::Added));
    }

    #[test]
    fn head_label_uses_branch_name_and_detached_short_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("seed.txt"), "seed\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        git(p, &["checkout", "-b", "feature"]);

        assert_eq!(resolve_head_label(p).unwrap(), "feature");

        let head = git_out(p, &["rev-parse", "HEAD"]);
        git(p, &["checkout", "--detach", "HEAD"]);

        assert_eq!(resolve_head_label(p).unwrap(), head[..7].to_string());
    }

    fn render_plain_letters(tree: &ChangeTree, root: &Path) -> String {
        let _c = crate::test_color::guard();
        colored::control::set_override(false);
        let lsc = lscolors::LsColors::empty();
        let out = TerminalRenderer {
            marks: MarkScheme::Letter,
            format: OutputFormat::Plain,
            ls_colors: &lsc,
            root: root.to_path_buf(),
            pr_header: None,
        }
        .render(tree)
        .unwrap();
        colored::control::unset_override();
        out
    }

    #[test]
    fn churn_counts_added_lines_for_tracked_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("seed.txt"), "seed\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        std::fs::write(p.join("added.txt"), "a\nb\nc\n").unwrap();
        git(p, &["add", "added.txt"]);
        let tree = collect_changes(p, ComparisonMode::Staged, false).unwrap().unwrap();
        let churn = find_file_churn(&tree, "added.txt").expect("added.txt present");
        assert_eq!(churn.added, 3, "three added lines counted");
        assert_eq!(churn.deleted, 0);
    }

    #[test]
    fn churn_counts_deleted_lines_for_tracked_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("gone.txt"), "1\n2\n3\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        git(p, &["rm", "gone.txt"]);
        let tree = collect_changes(p, ComparisonMode::Staged, false).unwrap().unwrap();
        let churn = find_file_churn(&tree, "gone.txt").expect("gone.txt present");
        assert_eq!(churn.added, 0);
        assert_eq!(churn.deleted, 3, "three deleted lines counted");
    }

    #[test]
    fn churn_counts_lines_for_untracked_files() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("seed.txt"), "seed\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        std::fs::write(p.join("new.txt"), "x\ny\n").unwrap(); // untracked, 2 lines
        let tree = collect_changes(p, ComparisonMode::Staged, true).unwrap().unwrap();
        let churn = find_file_churn(&tree, "new.txt").expect("new.txt present");
        assert_eq!(churn.added, 2, "two untracked lines counted");
        assert_eq!(churn.deleted, 0);
    }

    #[test]
    fn untracked_binary_file_has_zero_churn() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("seed.txt"), "seed\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        std::fs::write(p.join("binary.dat"), b"\0binary\n").unwrap();

        let tree = collect_changes(p, ComparisonMode::Staged, true).unwrap().unwrap();
        let node = find_file_node(&tree, "binary.dat").expect("binary.dat present");

        assert_eq!(node.status, ChangeStatus::Untracked);
        assert_eq!(node.churn, Churn::default());
    }

    #[test]
    fn untracked_invalid_utf8_file_has_zero_churn() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("seed.txt"), "seed\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        std::fs::write(p.join("invalid.txt"), [0xff, b'\n']).unwrap();

        let tree = collect_changes(p, ComparisonMode::Staged, true).unwrap().unwrap();
        let node = find_file_node(&tree, "invalid.txt").expect("invalid.txt present");

        assert_eq!(node.status, ChangeStatus::Untracked);
        assert_eq!(node.churn, Churn::default());
    }

    #[test]
    fn staged_rename_renders_as_single_rename() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("old-name.txt"), "same\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        git(p, &["mv", "old-name.txt", "new-name.txt"]);

        let tree = collect_changes(p, ComparisonMode::Staged, false).unwrap().unwrap();
        let out = render_plain_letters(&tree, p);

        assert!(out.contains("R old-name.txt -> new-name.txt"), "{out}");
        assert!(!out.contains("D old-name.txt"), "{out}");
        assert_eq!(tree.summary.files_changed, 1);
    }

    #[test]
    fn staged_copy_renders_as_single_copy_with_source_path() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("source.txt"), "same\nsame\nsame\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        std::fs::copy(p.join("source.txt"), p.join("copy.txt")).unwrap();
        git(p, &["add", "copy.txt"]);

        let tree = collect_changes(p, ComparisonMode::Staged, false).unwrap().unwrap();
        let out = render_plain_letters(&tree, p);

        assert!(out.contains("C source.txt => copy.txt"), "{out}");
        assert!(!out.contains("S copy.txt"), "{out}");
        assert_eq!(tree.summary.files_changed, 1);
        let node = find_file_node(&tree, "copy.txt").expect("copy.txt present");
        assert_eq!(node.status, ChangeStatus::Copied);
        assert_eq!(node.old_path.as_deref(), Some("source.txt"));
    }

    #[test]
    fn copied_display_name_uses_source_arrow() {
        let node = TreeNode {
            name: "copy.txt".to_string(),
            path: "copy.txt".to_string(),
            old_path: Some("source.txt".to_string()),
            kind: NodeKind::File,
            change_kind: Some(ChangeKind::Copied),
            status: ChangeStatus::Copied,
            churn: Churn::default(),
            rollup: Rollup::default(),
            children: vec![],
        };

        assert_eq!(display_name(&node), "source.txt => copy.txt");
    }

    #[test]
    fn copied_and_unreadable_marks_are_distinct() {
        let copied = TreeNode {
            name: "copy.txt".to_string(),
            path: "copy.txt".to_string(),
            old_path: Some("source.txt".to_string()),
            kind: NodeKind::File,
            change_kind: Some(ChangeKind::Copied),
            status: ChangeStatus::Copied,
            churn: Churn::default(),
            rollup: Rollup::default(),
            children: vec![],
        };
        let unreadable = TreeNode {
            name: "locked.txt".to_string(),
            path: "locked.txt".to_string(),
            old_path: None,
            kind: NodeKind::File,
            change_kind: Some(ChangeKind::Unreadable),
            status: ChangeStatus::Unreadable,
            churn: Churn::default(),
            rollup: Rollup::default(),
            children: vec![],
        };
        let conflict = TreeNode {
            name: "conflict.txt".to_string(),
            path: "conflict.txt".to_string(),
            old_path: None,
            kind: NodeKind::File,
            change_kind: Some(ChangeKind::Conflicted),
            status: ChangeStatus::Conflicted,
            churn: Churn::default(),
            rollup: Rollup::default(),
            children: vec![],
        };

        assert_eq!(mark(&copied, MarkScheme::Letter), "C");
        assert_eq!(mark(&copied, MarkScheme::Xy), "C ");
        assert_eq!(mark(&unreadable, MarkScheme::Letter), "E");
        assert_eq!(mark(&unreadable, MarkScheme::Xy), "E?");
        assert_eq!(mark(&conflict, MarkScheme::Letter), "U");
        assert_eq!(mark(&conflict, MarkScheme::Xy), "UU");
    }

    #[test]
    fn unreadable_delta_maps_to_unreadable_status() {
        assert_eq!(
            status_for_delta(git2::Delta::Unreadable, &ComparisonMode::Staged),
            ChangeStatus::Unreadable
        );
    }

    #[cfg(unix)]
    struct ModeGuard {
        path: PathBuf,
        mode: u32,
    }

    #[cfg(unix)]
    impl Drop for ModeGuard {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(self.mode));
        }
    }

    #[cfg(unix)]
    fn make_unreadable_or_skip(path: &Path) -> Option<ModeGuard> {
        use std::os::unix::fs::PermissionsExt;

        let original_mode = std::fs::metadata(path).unwrap().permissions().mode();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::File::open(path).is_ok() {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(original_mode));
            return None;
        }
        Some(ModeGuard { path: path.to_path_buf(), mode: original_mode })
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_untracked_file_renders_read_error() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("seed.txt"), "seed\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);

        let locked = p.join("locked.txt");
        std::fs::write(&locked, "secret\n").unwrap();
        let Some(_guard) = make_unreadable_or_skip(&locked) else { return };

        let tree = collect_changes(p, ComparisonMode::Staged, true).unwrap().unwrap();
        let out = render_plain_letters(&tree, p);

        assert!(out.contains("E locked.txt"), "{out}");
        assert!(!out.contains("? locked.txt"), "{out}");
        let node = find_file_node(&tree, "locked.txt").expect("locked.txt present");
        assert_eq!(node.status, ChangeStatus::Unreadable);
        assert_eq!(node.churn, Churn::default());
    }

    #[cfg(unix)]
    #[test]
    fn tracked_unreadable_file_renders_read_error() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        let tracked = p.join("tracked.txt");
        std::fs::write(&tracked, "tracked\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);

        let Some(_guard) = make_unreadable_or_skip(&tracked) else { return };

        let tree = collect_changes(p, ComparisonMode::Unstaged, false).unwrap().unwrap();
        let out = render_plain_letters(&tree, p);

        assert!(out.contains("E tracked.txt"), "{out}");
        let node = find_file_node(&tree, "tracked.txt").expect("tracked.txt present");
        assert_eq!(node.status, ChangeStatus::Unreadable);
        assert_eq!(node.churn, Churn::default());
    }

    #[cfg(unix)]
    #[test]
    fn tracked_unreadable_file_does_not_hide_untracked_files() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        let tracked = p.join("tracked.txt");
        std::fs::write(&tracked, "tracked\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        std::fs::write(p.join("untracked.txt"), "new\n").unwrap();

        let Some(_guard) = make_unreadable_or_skip(&tracked) else { return };

        let tree = collect_changes(p, ComparisonMode::Unstaged, true).unwrap().unwrap();
        let out = render_plain_letters(&tree, p);

        assert!(out.contains("E tracked.txt"), "{out}");
        assert!(out.contains("? untracked.txt"), "{out}");
        assert_eq!(
            find_file_node(&tree, "untracked.txt").expect("untracked.txt present").status,
            ChangeStatus::Untracked
        );
    }

    #[test]
    fn staged_typechange_renders_with_typechange_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("typechange.txt"), "plain\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        std::fs::remove_file(p.join("typechange.txt")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("target.txt", p.join("typechange.txt")).unwrap();
        #[cfg(not(unix))]
        std::fs::write(p.join("typechange.txt"), "target.txt\n").unwrap();
        git(p, &["add", "-A"]);

        let tree = collect_changes(p, ComparisonMode::Staged, false).unwrap().unwrap();
        let out = render_plain_letters(&tree, p);

        #[cfg(unix)]
        assert!(out.contains("T typechange.txt"), "{out}");
        #[cfg(not(unix))]
        assert!(out.contains("S typechange.txt"), "{out}");
    }

    #[test]
    fn merge_conflict_renders_with_conflict_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("conflict.txt"), "base\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "base"]);
        git(p, &["branch", "-M", "main"]);
        git(p, &["checkout", "-b", "feature"]);
        std::fs::write(p.join("conflict.txt"), "feature\n").unwrap();
        git(p, &["commit", "-am", "feature"]);
        git(p, &["checkout", "main"]);
        std::fs::write(p.join("conflict.txt"), "master\n").unwrap();
        git(p, &["commit", "-am", "master"]);
        git_fails(p, &["merge", "feature"]);

        let tree = collect_changes(p, ComparisonMode::Uncommitted, true).unwrap().unwrap();
        let out = render_plain_letters(&tree, p);

        assert!(out.contains("U conflict.txt"), "{out}");
    }

    /// Sets up: c0 (base.txt) on base branch; a `feature` branch with feat.txt;
    /// a base-only commit main2.txt that feature never sees; and an untracked
    /// working.txt in feature's worktree. Returns (tmpdir, c0_sha).
    fn setup_pr_repo() -> (tempfile::TempDir, String) {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("base.txt"), "x").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        let c0 = git_out(p, &["rev-parse", "HEAD"]);
        let base_branch = git_out(p, &["symbolic-ref", "--short", "HEAD"]);
        git(p, &["checkout", "-b", "feature"]);
        std::fs::write(p.join("feat.txt"), "y").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "feat"]);
        git(p, &["checkout", &base_branch]);
        std::fs::write(p.join("main2.txt"), "z").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "main2"]);
        git(p, &["checkout", "feature"]);
        std::fs::write(p.join("working.txt"), "w").unwrap(); // untracked
        (tmp, c0)
    }

    #[test]
    fn pr_committed_shows_only_branch_commits() {
        let (tmp, c0) = setup_pr_repo();
        let mode = ComparisonMode::Pr { merge_base: c0, committed: true };
        let tree = collect_changes(tmp.path(), mode, false).unwrap().unwrap();
        let names = file_names(&tree);
        assert!(names.iter().any(|n| n == "feat.txt"), "branch commit shown");
        assert!(!names.iter().any(|n| n == "main2.txt"), "base-only commit excluded");
        assert!(!names.iter().any(|n| n == "working.txt"), "uncommitted excluded");
    }

    #[test]
    fn pr_default_includes_working_tree_and_untracked() {
        let (tmp, c0) = setup_pr_repo();
        let mode = ComparisonMode::Pr { merge_base: c0, committed: false };
        let tree = collect_changes(tmp.path(), mode, true).unwrap().unwrap();
        let names = file_names(&tree);
        assert!(names.iter().any(|n| n == "feat.txt"), "branch commit shown");
        assert!(names.iter().any(|n| n == "working.txt"), "untracked shown");
        assert!(!names.iter().any(|n| n == "main2.txt"), "base-only commit excluded");
    }

    #[test]
    fn pr_default_marks_worktree_edits_as_unstaged() {
        let (tmp, c0) = setup_pr_repo();
        let p = tmp.path();
        std::fs::write(p.join("base.txt"), "edited\n").unwrap();

        let mode = ComparisonMode::Pr { merge_base: c0, committed: false };
        let tree = collect_changes(p, mode, false).unwrap().unwrap();
        let out = render_plain_letters(&tree, p);
        let node = find_file_node(&tree, "base.txt").expect("base.txt present");

        assert_eq!(node.status, ChangeStatus::Unstaged);
        assert!(out.contains("M base.txt"), "{out}");
    }

    #[test]
    fn pr_all_view_excludes_untracked_when_committed() {
        let (tmp, c0) = setup_pr_repo();
        let opts = WalkOpts { all: false, gitignore: false, level: None, dirs_only: false };
        let mode = ComparisonMode::Pr { merge_base: c0, committed: true };
        let tree = collect_all_files(tmp.path(), mode, opts).unwrap().unwrap();
        let names = file_names(&tree);
        // all-files view lists every file, but untracked must NOT appear under --committed
        assert!(names.iter().any(|n| n == "base.txt"), "unchanged file listed in all-files view");
        assert!(!names.iter().any(|n| n == "working.txt"), "untracked excluded under committed");
    }

    #[test]
    fn pr_all_view_excludes_ignored_files_when_committed() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join(".gitignore"), "ignored.log\n").unwrap();
        std::fs::write(p.join("base.txt"), "base\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        git(p, &["branch", "-M", "main"]);
        let c0 = git_out(p, &["rev-parse", "HEAD"]);
        git(p, &["checkout", "-b", "feature"]);
        std::fs::write(p.join("feature.txt"), "feature\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "feature"]);
        std::fs::write(p.join("ignored.log"), "ignored\n").unwrap();

        let opts = WalkOpts { all: false, gitignore: false, level: None, dirs_only: false };
        let mode = ComparisonMode::Pr { merge_base: c0, committed: true };
        let tree = collect_all_files(p, mode, opts).unwrap().unwrap();
        let names = file_names(&tree);

        assert!(names.iter().any(|n| n == "feature.txt"), "branch file shown");
        assert!(!names.iter().any(|n| n == "ignored.log"), "ignored worktree file excluded");
    }

    #[test]
    fn pr_all_view_excludes_untracked_only_dirs_when_committed() {
        let (tmp, c0) = setup_pr_repo();
        let p = tmp.path();
        std::fs::create_dir(p.join("scratch")).unwrap();
        std::fs::write(p.join("scratch/u.txt"), "untracked\n").unwrap();

        let opts = WalkOpts { all: false, gitignore: false, level: None, dirs_only: false };
        let mode = ComparisonMode::Pr { merge_base: c0, committed: true };
        let tree = collect_all_files(p, mode, opts).unwrap().unwrap();
        let dirs = dir_paths(&tree);

        assert!(!dirs.iter().any(|d| d == "scratch"), "untracked-only dir excluded");
    }

    #[test]
    fn pr_all_dirs_only_keeps_tracked_dirs_when_pruning_untracked_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::create_dir(p.join("src")).unwrap();
        std::fs::write(p.join("src/base.txt"), "base\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        git(p, &["branch", "-M", "main"]);
        let c0 = git_out(p, &["rev-parse", "HEAD"]);
        git(p, &["checkout", "-b", "feature"]);
        std::fs::write(p.join("src/feature.txt"), "feature\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "feature"]);
        std::fs::create_dir(p.join("scratch")).unwrap();
        std::fs::write(p.join("scratch/u.txt"), "untracked\n").unwrap();

        let opts = WalkOpts { all: false, gitignore: false, level: None, dirs_only: true };
        let mode = ComparisonMode::Pr { merge_base: c0, committed: true };
        let tree = collect_all_files(p, mode, opts).unwrap().unwrap();
        let dirs = dir_paths(&tree);

        assert!(dirs.iter().any(|d| d == "src"), "tracked dir kept");
        assert!(!dirs.iter().any(|d| d == "scratch"), "untracked-only dir excluded");
    }

    #[test]
    fn pr_all_view_includes_deleted_files() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("keep.txt"), "keep\n").unwrap();
        std::fs::write(p.join("gone.txt"), "gone\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        git(p, &["branch", "-M", "main"]);
        let c0 = git_out(p, &["rev-parse", "HEAD"]);
        git(p, &["checkout", "-b", "feature"]);
        git(p, &["rm", "gone.txt"]);
        git(p, &["commit", "-m", "delete"]);

        let opts = WalkOpts { all: false, gitignore: false, level: None, dirs_only: false };
        let mode = ComparisonMode::Pr { merge_base: c0, committed: false };
        let tree = collect_all_files(p, mode, opts).unwrap().unwrap();
        let names = file_names(&tree);

        assert!(names.iter().any(|n| n == "gone.txt"), "deleted file listed in all-files view");
        assert_eq!(tree.summary.files_changed, 1, "deleted file counted as changed");
    }

    #[test]
    fn scoped_pr_committed_includes_file_moved_out_as_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::create_dir(p.join("src")).unwrap();
        std::fs::write(p.join("src/a.txt"), "a\n").unwrap();
        std::fs::write(p.join("src/keep.txt"), "keep\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        git(p, &["branch", "-M", "main"]);
        let c0 = git_out(p, &["rev-parse", "HEAD"]);
        git(p, &["checkout", "-b", "feature"]);
        std::fs::create_dir(p.join("other")).unwrap();
        git(p, &["mv", "src/a.txt", "other/a.txt"]);
        git(p, &["commit", "-m", "move out"]);

        let mode = ComparisonMode::Pr { merge_base: c0, committed: true };
        let tree = collect_changes(&p.join("src"), mode, false).unwrap().unwrap();
        let out = render_plain_letters(&tree, &p.join("src"));
        let node = find_file_node(&tree, "a.txt").expect("moved-out file present");

        assert_eq!(node.status, ChangeStatus::Deleted);
        assert_eq!(node.path, "a.txt");
        assert!(out.contains("D a.txt"), "{out}");
        assert!(!out.contains("other/a.txt"), "{out}");
    }

    #[test]
    fn scoped_rename_moved_out_normalizes_scope_separators() {
        let change = FileChange {
            path: PathBuf::from("other/a.txt"),
            old_path: Some(PathBuf::from("src/nested/a.txt")),
            status: ChangeStatus::Renamed,
            change_kind: Some(ChangeKind::Renamed),
            churn: Churn::default(),
        };

        let scoped = scoped_file_change(change, Path::new("src\\nested")).unwrap();

        assert_eq!(scoped.status, ChangeStatus::Deleted);
        assert_eq!(scoped.path, PathBuf::from("a.txt"));
        assert_eq!(scoped.old_path, None);
    }

    #[test]
    fn scope_relative_path_handles_windows_verbatim_prefix() {
        let scope = Path::new(r"\\?\C:\runner\_work\difftree\difftree\src");
        let workdir = Path::new(r"C:\runner\_work\difftree\difftree");

        assert_eq!(scope_relative_path(scope, workdir), PathBuf::from("src"));
    }

    /// Repo with a `main` branch (c0) and a `feature` branch (c0 + feat).
    /// Returns (tmpdir, c0_sha).
    fn setup_named_repo(base: &str) -> (tempfile::TempDir, String) {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("base.txt"), "x").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        git(p, &["branch", "-M", base]);
        let c0 = git_out(p, &["rev-parse", "HEAD"]);
        git(p, &["checkout", "-b", "feature"]);
        std::fs::write(p.join("feat.txt"), "y").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "feat"]);
        (tmp, c0)
    }

    #[test]
    fn resolve_auto_detects_main() {
        let (tmp, c0) = setup_named_repo("main");
        let b = resolve_pr_base(tmp.path(), None).unwrap();
        assert_eq!(b.base_name, "main");
        assert_eq!(b.merge_base, c0);
        assert!(!b.on_base);
    }

    #[test]
    fn resolve_falls_through_to_master() {
        let (tmp, _c0) = setup_named_repo("master");
        let b = resolve_pr_base(tmp.path(), None).unwrap();
        assert_eq!(b.base_name, "master");
    }

    #[test]
    fn resolve_bad_override_errors() {
        let (tmp, _c0) = setup_named_repo("main");
        let err = resolve_pr_base(tmp.path(), Some("no-such-ref")).unwrap_err();
        assert!(err.to_string().contains("could not resolve base branch"));
    }

    #[test]
    fn resolve_on_base_branch_sets_flag() {
        let (tmp, c0) = setup_named_repo("main");
        git(tmp.path(), &["checkout", "main"]);
        let b = resolve_pr_base(tmp.path(), None).unwrap();
        assert_eq!(b.merge_base, c0);
        assert!(b.on_base);
    }

    #[test]
    fn resolve_prefers_remote_tracking_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("base.txt"), "x").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        git(p, &["branch", "-M", "main"]);
        let c0 = git_out(p, &["rev-parse", "HEAD"]);
        // Fabricate a remote-tracking ref at c0 without a real remote.
        git(p, &["update-ref", "refs/remotes/origin/main", &c0]);
        // Advance local main past origin/main.
        std::fs::write(p.join("base.txt"), "x2").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c1"]);
        git(p, &["checkout", "-b", "feature"]);
        std::fs::write(p.join("feat.txt"), "y").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "feat"]);

        let b = resolve_pr_base(p, None).unwrap();
        assert_eq!(b.base_ref, "origin/main", "remote-tracking ref preferred over local");
        assert_eq!(b.merge_base, c0, "merge-base taken against origin/main (c0)");
    }

    #[test]
    fn resolve_explicit_override_prefers_exact_ref_over_origin() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::write(p.join("base.txt"), "x").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);
        git(p, &["branch", "-M", "main"]);
        let c0 = git_out(p, &["rev-parse", "HEAD"]);
        git(p, &["update-ref", "refs/remotes/origin/main", &c0]);

        std::fs::write(p.join("local-main.txt"), "x2").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "local-main"]);
        let local_main = git_out(p, &["rev-parse", "HEAD"]);
        git(p, &["checkout", "-b", "feature"]);
        std::fs::write(p.join("feat.txt"), "y").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "feat"]);

        let b = resolve_pr_base(p, Some("main")).unwrap();
        assert_eq!(b.base_ref, "main", "explicit ref should be resolved exactly");
        assert_eq!(b.merge_base, local_main, "merge-base taken against local main");
    }
}

/// Renders a filename with an LsColors style applied (foreground color +
/// bold/italic/underline). Goes through the `colored` crate, so it honors the
/// global color override / TTY detection: when color is disabled the result is
/// the plain name.
fn xterm_fixed_color_to_rgb(n: u8) -> (u8, u8, u8) {
    const ANSI16: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (128, 0, 0),
        (0, 128, 0),
        (128, 128, 0),
        (0, 0, 128),
        (128, 0, 128),
        (0, 128, 128),
        (192, 192, 192),
        (128, 128, 128),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (0, 0, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    if n < 16 {
        return ANSI16[n as usize];
    }
    if n < 232 {
        let n = n - 16;
        let scale = [0, 95, 135, 175, 215, 255];
        return (scale[(n / 36) as usize], scale[((n / 6) % 6) as usize], scale[(n % 6) as usize]);
    }
    let gray = 8 + (n - 232) * 10;
    (gray, gray, gray)
}

pub fn style_name(name: &str, style: &lscolors::Style) -> String {
    let mut styled = name.normal();
    if let Some(fg) = &style.foreground {
        use lscolors::Color as LsColor;
        let color = match fg {
            LsColor::Black => colored::Color::Black,
            LsColor::Red => colored::Color::Red,
            LsColor::Green => colored::Color::Green,
            LsColor::Yellow => colored::Color::Yellow,
            LsColor::Blue => colored::Color::Blue,
            LsColor::Magenta => colored::Color::Magenta,
            LsColor::Cyan => colored::Color::Cyan,
            LsColor::White => colored::Color::White,
            LsColor::BrightBlack => colored::Color::BrightBlack,
            LsColor::BrightRed => colored::Color::BrightRed,
            LsColor::BrightGreen => colored::Color::BrightGreen,
            LsColor::BrightYellow => colored::Color::BrightYellow,
            LsColor::BrightBlue => colored::Color::BrightBlue,
            LsColor::BrightMagenta => colored::Color::BrightMagenta,
            LsColor::BrightCyan => colored::Color::BrightCyan,
            LsColor::BrightWhite => colored::Color::BrightWhite,
            LsColor::Fixed(n) => {
                let (r, g, b) = xterm_fixed_color_to_rgb(*n);
                colored::Color::TrueColor { r, g, b }
            }
            LsColor::RGB(r, g, b) => colored::Color::TrueColor { r: *r, g: *g, b: *b },
        };
        styled = styled.color(color);
    }
    if style.font_style.bold {
        styled = styled.bold();
    }
    if style.font_style.italic {
        styled = styled.italic();
    }
    if style.font_style.underline {
        styled = styled.underline();
    }
    styled.to_string()
}

#[cfg(test)]
pub(crate) mod test_color {
    use std::sync::{Mutex, MutexGuard};
    static LOCK: Mutex<()> = Mutex::new(());
    /// Serializes tests that mutate the global `colored` override.
    pub fn guard() -> MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod style_name_tests {
    use super::*;

    struct EnvVarGuard {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let original = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.original {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    #[test]
    fn style_name_applies_foreground_when_color_on() {
        let _c = crate::test_color::guard();
        colored::control::set_override(true);
        let style =
            lscolors::Style { foreground: Some(lscolors::Color::Green), ..Default::default() };
        let out = style_name("file.rs", &style);
        assert!(out.contains("\x1b[32m"), "green ANSI present when color on: {out:?}");
        assert!(out.contains("file.rs"));
        colored::control::unset_override();
    }

    #[test]
    fn style_name_plain_when_color_off() {
        let _c = crate::test_color::guard();
        colored::control::set_override(false);
        let style =
            lscolors::Style { foreground: Some(lscolors::Color::Green), ..Default::default() };
        let out = style_name("file.rs", &style);
        assert_eq!(out, "file.rs", "plain when color off");
        colored::control::unset_override();
    }

    #[test]
    fn style_name_preserves_fixed_256_color_as_truecolor() {
        let _c = crate::test_color::guard();
        colored::control::set_override(true);
        let _env = EnvVarGuard::set("COLORTERM", "truecolor");
        let style =
            lscolors::Style { foreground: Some(lscolors::Color::Fixed(208)), ..Default::default() };
        let out = style_name("file.rs", &style);
        assert!(
            out.contains("\x1b[38;2;255;135;0m"),
            "fixed 208 approximated to orange truecolor: {out:?}"
        );
        colored::control::unset_override();
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;

    fn sample_tree() -> ChangeTree {
        let staged = TreeNode {
            name: "a.rs".into(),
            path: "a.rs".into(),
            old_path: None,
            kind: NodeKind::File,
            change_kind: Some(ChangeKind::Added),
            status: ChangeStatus::Staged,
            churn: Churn { added: 3, deleted: 1 },
            rollup: Rollup::default(),
            children: vec![],
        };
        let deleted = TreeNode {
            name: "gone.rs".into(),
            path: "gone.rs".into(),
            old_path: None,
            kind: NodeKind::File,
            change_kind: Some(ChangeKind::Deleted),
            status: ChangeStatus::Deleted,
            churn: Churn { added: 0, deleted: 5 },
            rollup: Rollup::default(),
            children: vec![],
        };
        let summary =
            Rollup { dirs_touched: 0, files_changed: 2, churn: Churn { added: 3, deleted: 6 } };
        let root = TreeNode {
            name: "repo".into(),
            path: "".into(),
            old_path: None,
            kind: NodeKind::Directory,
            change_kind: None,
            status: ChangeStatus::Clean,
            churn: Churn::default(),
            rollup: summary.clone(),
            children: vec![staged, deleted],
        };
        ChangeTree {
            schema_version: SCHEMA_VERSION.into(),
            comparison: ComparisonMode::Staged,
            view: View::BlastRadius,
            root,
            summary,
            fallback: None,
        }
    }

    #[test]
    fn color_on_emits_ansi_for_marks_and_churn() {
        let _c = crate::test_color::guard();
        colored::control::set_override(true);
        let lsc = lscolors::LsColors::empty();
        let r = TerminalRenderer {
            marks: MarkScheme::Symbol,
            format: OutputFormat::Pretty,
            ls_colors: &lsc,
            root: std::path::PathBuf::from("/no/such/root"),
            pr_header: None,
        };
        let out = r.render(&sample_tree()).unwrap();
        assert!(out.contains("\x1b[32m"), "green present (staged mark / +N): {out:?}");
        assert!(out.contains("\x1b[31m"), "red present (deleted mark / −M): {out:?}");
        assert!(
            out.contains("\x1b[1;2m") || out.contains("\x1b[2;1m"),
            "header bold+dim ANSI present: {out:?}"
        );
        colored::control::unset_override();
    }

    #[test]
    fn color_off_is_plain() {
        let _c = crate::test_color::guard();
        colored::control::set_override(false);
        let lsc = lscolors::LsColors::empty();
        let r = TerminalRenderer {
            marks: MarkScheme::Symbol,
            format: OutputFormat::Pretty,
            ls_colors: &lsc,
            root: std::path::PathBuf::from("/no/such/root"),
            pr_header: None,
        };
        let out = r.render(&sample_tree()).unwrap();
        assert!(!out.contains("\x1b["), "no ANSI when color off: {out:?}");
        assert!(out.starts_with("Staged changes\nrepo\n"), "{out:?}");
        colored::control::unset_override();
    }

    #[test]
    fn filenames_use_lscolors() {
        let _c = crate::test_color::guard();
        colored::control::set_override(true);
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.rs"), "x").unwrap();
        let lsc = lscolors::LsColors::from_string("*.rs=35"); // magenta foreground
        let r = TerminalRenderer {
            marks: MarkScheme::Symbol,
            format: OutputFormat::Pretty,
            ls_colors: &lsc,
            root: tmp.path().to_path_buf(),
            pr_header: None,
        };
        let out = r.render(&sample_tree()).unwrap();
        assert!(out.contains("\x1b[35m"), "filename magenta from LS_COLORS: {out:?}");
        colored::control::unset_override();
    }

    #[test]
    fn deleted_missing_path_does_not_panic() {
        let _c = crate::test_color::guard();
        colored::control::set_override(true);
        let lsc = lscolors::LsColors::empty();
        let r = TerminalRenderer {
            marks: MarkScheme::Symbol,
            format: OutputFormat::Pretty,
            ls_colors: &lsc,
            root: std::path::PathBuf::from("/no/such/root"),
            pr_header: None,
        };
        // gone.rs does not exist under root → style lookup must fall back, not panic.
        let _ = r.render(&sample_tree()).unwrap();
        colored::control::unset_override();
    }
}

#[cfg(test)]
mod summary_frame_tests {
    use super::*;

    fn file(name: &str, status: ChangeStatus, change_kind: ChangeKind) -> TreeNode {
        TreeNode {
            name: name.into(),
            path: name.into(),
            old_path: None,
            kind: NodeKind::File,
            change_kind: Some(change_kind),
            status,
            churn: Churn { added: 1, deleted: 0 },
            rollup: Rollup {
                dirs_touched: 0,
                files_changed: 1,
                churn: Churn { added: 1, deleted: 0 },
            },
            children: vec![],
        }
    }

    fn tree_for(mode: ComparisonMode, children: Vec<TreeNode>) -> ChangeTree {
        let mut summary = Rollup::default();
        for child in &children {
            summary.files_changed += child.rollup.files_changed;
            summary.churn.added += child.rollup.churn.added;
            summary.churn.deleted += child.rollup.churn.deleted;
        }
        let root = TreeNode {
            name: "repo".into(),
            path: "".into(),
            old_path: None,
            kind: NodeKind::Directory,
            change_kind: None,
            status: ChangeStatus::Clean,
            churn: Churn::default(),
            rollup: summary.clone(),
            children,
        };
        ChangeTree {
            schema_version: SCHEMA_VERSION.into(),
            comparison: mode,
            view: View::BlastRadius,
            root,
            summary,
            fallback: None,
        }
    }

    fn render_plain(tree: &ChangeTree, pr_header: Option<PrHeaderContext>) -> String {
        let _c = crate::test_color::guard();
        colored::control::set_override(false);
        let lsc = lscolors::LsColors::empty();
        let out = TerminalRenderer {
            marks: MarkScheme::Letter,
            format: OutputFormat::Plain,
            ls_colors: &lsc,
            root: std::path::PathBuf::from("/repo"),
            pr_header,
        }
        .render(tree)
        .unwrap();
        colored::control::unset_override();
        out
    }

    #[test]
    fn renders_header_text_for_each_comparison_mode() {
        let staged = tree_for(ComparisonMode::Staged, vec![]);
        assert!(render_plain(&staged, None).starts_with("Staged changes\nrepo\n"));

        let unstaged = tree_for(ComparisonMode::Unstaged, vec![]);
        assert!(render_plain(&unstaged, None).starts_with("Unstaged changes\nrepo\n"));

        let uncommitted = tree_for(ComparisonMode::Uncommitted, vec![]);
        assert!(render_plain(&uncommitted, None)
            .starts_with("Uncommitted changes (staged + unstaged)\nrepo\n"));

        let against = tree_for(ComparisonMode::Against { reference: "origin/main".into() }, vec![]);
        assert!(render_plain(&against, None).starts_with("Against: origin/main...working tree\n"));

        let range = tree_for(ComparisonMode::Range { range: "HEAD~2..HEAD".into() }, vec![]);
        assert!(render_plain(&range, None).starts_with("Range: HEAD~2..HEAD\n"));
    }

    #[test]
    fn renders_pr_header_variants() {
        let working = tree_for(
            ComparisonMode::Pr { merge_base: "3589ffcabcdef".into(), committed: false },
            vec![],
        );
        let committed = tree_for(
            ComparisonMode::Pr { merge_base: "3589ffcabcdef".into(), committed: true },
            vec![],
        );
        let header = PrHeaderContext {
            base_ref: "origin/main".into(),
            head_label: "feature".into(),
            on_base: false,
        };

        assert!(render_plain(&working, Some(header.clone()))
            .starts_with("PR: origin/main...feature · working tree\nrepo\n"));
        assert!(render_plain(&committed, Some(header.clone()))
            .starts_with("PR: origin/main...feature · committed\nrepo\n"));

        let on_base = PrHeaderContext {
            base_ref: "origin/main".into(),
            head_label: "main".into(),
            on_base: true,
        };
        assert!(render_plain(&working, Some(on_base))
            .starts_with("PR: origin/main...main · on base branch (uncommitted only)\nrepo\n"));
    }

    #[test]
    fn footer_collapses_single_kind_and_breaks_down_mixed_kinds() {
        let single = tree_for(
            ComparisonMode::Staged,
            vec![
                file("a.rs", ChangeStatus::Staged, ChangeKind::Modified),
                file("b.rs", ChangeStatus::Staged, ChangeKind::Modified),
            ],
        );
        let single_out = render_plain(&single, None);
        assert!(single_out.contains("0 dirs touched · 2 files modified · +2 −0"), "{single_out}");
        assert!(!single_out.contains("(2 modified)"), "{single_out}");

        let mixed = tree_for(
            ComparisonMode::Staged,
            vec![
                file("a.rs", ChangeStatus::Staged, ChangeKind::Added),
                file("b.rs", ChangeStatus::Staged, ChangeKind::Modified),
                file("c.rs", ChangeStatus::Deleted, ChangeKind::Deleted),
            ],
        );
        let mixed_out = render_plain(&mixed, None);
        assert!(
            mixed_out.contains(
                "0 dirs touched · 3 files changed (1 added · 1 modified · 1 deleted) · +3 −0"
            ),
            "{mixed_out}"
        );
    }

    #[test]
    fn footer_omits_zero_kinds_for_empty_change_sets() {
        let empty = tree_for(ComparisonMode::Staged, vec![]);
        let out = render_plain(&empty, None);

        assert!(out.contains("0 dirs touched · 0 files changed · +0 −0"), "{out}");
        assert!(!out.contains("()"), "{out}");
    }
}

#[cfg(test)]
mod all_files_tests {
    use super::*;
    use std::process::Command as Pcmd;

    fn git(dir: &Path, args: &[&str]) {
        let output = Pcmd::new("git").args(args).current_dir(dir).output().unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn all_files_view_includes_unchanged_files() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        std::fs::create_dir(p.join("src")).unwrap();
        std::fs::create_dir(p.join("docs")).unwrap();
        std::fs::write(p.join("src/changed.rs"), "a").unwrap();
        std::fs::write(p.join("docs/readme.md"), "b").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "init"]);
        std::fs::write(p.join("src/changed.rs"), "a2").unwrap();
        git(p, &["add", "src/changed.rs"]);

        let opts = WalkOpts { all: false, gitignore: false, level: None, dirs_only: false };
        let tree = collect_all_files(p, ComparisonMode::Staged, opts).unwrap().unwrap();
        assert_eq!(tree.view, View::AllFiles);

        // Collect every file name present in the tree.
        fn names(n: &TreeNode, out: &mut Vec<(String, ChangeStatus)>) {
            if n.kind == NodeKind::File {
                out.push((n.name.clone(), n.status.clone()));
            }
            for c in &n.children {
                names(c, out);
            }
        }
        let mut files = Vec::new();
        names(&tree.root, &mut files);
        assert!(files.iter().any(|(n, _)| n == "readme.md"), "unchanged file must appear");
        let changed = files.iter().find(|(n, _)| n == "changed.rs").expect("changed file present");
        assert_eq!(changed.1, ChangeStatus::Staged);
        let unchanged = files.iter().find(|(n, _)| n == "readme.md").unwrap();
        assert_eq!(unchanged.1, ChangeStatus::Clean);
    }
}
