//! Core difftree library: serializable model, git-backed collection, and renderers.

use colored::Colorize;
use git2::{DiffFindOptions, DiffOptions, Repository};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: &str = "difftree.v1";

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub kind: NodeKind,
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
        out.push_str(&format!("{}\n", tree.root.name));
        for (idx, child) in tree.root.children.iter().enumerate() {
            self.node(&mut out, child, "", idx + 1 == tree.root.children.len());
        }
        out.push_str(&format!(
            "\n{} dirs touched · {} files changed · {} {}\n",
            tree.summary.dirs_touched,
            tree.summary.files_changed,
            add_str(tree.summary.churn.added),
            del_str(tree.summary.churn.deleted)
        ));
        Ok(out)
    }
}
impl TerminalRenderer<'_> {
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
    let scope_rel = scope.strip_prefix(workdir).unwrap_or(Path::new(""));
    let effective = mode.clone();
    let mut files = diff_files(&repo, &effective)?;
    if includes_worktree_statuses(&mode) {
        add_conflicted(&repo, &mut files)?;
    }
    if include_untracked {
        add_untracked(&repo, &mut files)?;
    }
    files.retain(|f| scope_rel.as_os_str().is_empty() || f.path.starts_with(scope_rel));
    if !scope_rel.as_os_str().is_empty() {
        for f in &mut files {
            if let Ok(rel) = f.path.strip_prefix(scope_rel) {
                f.path = rel.to_path_buf();
            }
            if let Some(old_path) = &mut f.old_path {
                if let Ok(rel) = old_path.strip_prefix(scope_rel) {
                    *old_path = rel.to_path_buf();
                }
            }
        }
    }
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
    diff: &git2::Diff<'_>,
    mode: &ComparisonMode,
) -> anyhow::Result<Vec<FileChange>> {
    let mut out = Vec::new();
    for idx in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(idx) else { continue };
        let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) else {
            continue;
        };
        let status = status_for_delta(delta.status(), mode);
        let old_path = if matches!(status, ChangeStatus::Renamed | ChangeStatus::Copied) {
            delta.old_file().path().map(|p| p.to_path_buf())
        } else {
            None
        };
        // Per-file line stats; binary or patch-less deltas have no textual churn.
        let churn = if status == ChangeStatus::Unreadable {
            Churn::default()
        } else {
            match git2::Patch::from_diff(&diff, idx)? {
                Some(patch) => {
                    let (_context, added, deleted) = patch.line_stats()?;
                    Churn { added, deleted }
                }
                None => Churn::default(),
            }
        };
        out.push(FileChange { path: path.to_path_buf(), old_path, status, churn });
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
        return build_diff(repo, mode).and_then(|diff| file_changes_from_diff(&diff, mode));
    }

    let readable_paths: BTreeSet<PathBuf> =
        tracked_paths.difference(&unreadable_paths).cloned().collect();
    let mut out = if readable_paths.is_empty() {
        Vec::new()
    } else {
        let diff = build_diff_for_paths(repo, mode, Some(&readable_paths))?;
        file_changes_from_diff(&diff, mode)?
    };

    let existing: BTreeSet<PathBuf> = out.iter().map(|f| f.path.clone()).collect();
    for path in unreadable_paths {
        if !existing.contains(&path) {
            out.push(FileChange {
                path,
                old_path: None,
                status: ChangeStatus::Unreadable,
                churn: Churn::default(),
            });
        }
    }
    Ok(out)
}

fn diff_files(repo: &Repository, mode: &ComparisonMode) -> anyhow::Result<Vec<FileChange>> {
    match build_diff(repo, mode) {
        Ok(diff) => file_changes_from_diff(&diff, mode),
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
                        churn: Churn::default(),
                    });
                }
            }
        }
    }
    Ok(())
}
fn status_and_churn_for_untracked(abs: &Path) -> (ChangeStatus, Churn) {
    match std::fs::read(abs) {
        Ok(bytes) => {
            if bytes.contains(&0) {
                return (ChangeStatus::Untracked, Churn::default());
            }
            let added = String::from_utf8(bytes).map(|s| s.lines().count()).unwrap_or(0);
            (ChangeStatus::Untracked, Churn { added, deleted: 0 })
        }
        Err(_) => (ChangeStatus::Unreadable, Churn::default()),
    }
}

fn add_untracked_from_workdir_walk(
    repo: &Repository,
    files: &mut Vec<FileChange>,
) -> anyhow::Result<()> {
    let Some(workdir) = repo.workdir() else {
        return Ok(());
    };
    let mut tracked = BTreeSet::new();
    add_index_paths(repo, &mut tracked)?;
    let mut existing: BTreeSet<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
    let mut builder = ignore::WalkBuilder::new(workdir);
    builder.hidden(false).git_ignore(true).filter_entry(|entry| entry.file_name() != ".git");
    for result in builder.build() {
        let Ok(entry) = result else { continue };
        if entry.depth() == 0 || entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(workdir) else { continue };
        let rel = rel.to_path_buf();
        if tracked.contains(&rel) || existing.contains(&rel) {
            continue;
        }
        if repo.status_should_ignore(&rel).unwrap_or(false) {
            continue;
        }
        let (status, churn) = status_and_churn_for_untracked(entry.path());
        files.push(FileChange { path: rel.clone(), old_path: None, status, churn });
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
                let (status, churn) = workdir
                    .map(|w| status_and_churn_for_untracked(&w.join(p)))
                    .unwrap_or((ChangeStatus::Unreadable, Churn::default()));
                files.push(FileChange { path: PathBuf::from(p), old_path: None, status, churn });
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
    let scope_rel = scope.strip_prefix(workdir).unwrap_or(Path::new("")).to_path_buf();

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
    for mut f in changed {
        let rel = if scope_rel.as_os_str().is_empty() {
            Some(f.path.clone())
        } else {
            f.path.strip_prefix(&scope_rel).ok().map(|r| r.to_path_buf())
        };
        if let Some(rel) = rel {
            if let Some(old_path) = &mut f.old_path {
                if !scope_rel.as_os_str().is_empty() {
                    if let Ok(old_rel) = old_path.strip_prefix(&scope_rel) {
                        *old_path = old_rel.to_path_buf();
                    }
                }
            }
            f.path = rel.clone();
            change_map.insert(rel, f);
        }
    }

    // For Pr committed mode, build a set of untracked paths to exclude from the
    // filesystem walk (the change_map guard above only controls status labels,
    // not which entries the walker surfaces).
    let untracked_rel: BTreeSet<PathBuf> =
        if matches!(mode, ComparisonMode::Pr { committed: true, .. }) {
            let mut u = Vec::new();
            add_untracked(&repo, &mut u)?;
            u.into_iter()
                .filter_map(|f| {
                    if scope_rel.as_os_str().is_empty() {
                        Some(f.path)
                    } else {
                        f.path.strip_prefix(&scope_rel).ok().map(|r| r.to_path_buf())
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
            dirset.insert(rel);
        } else {
            if untracked_rel.contains(&rel) {
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
                churn: Churn::default(),
            });
            fmap.insert(rel, fc);
        }
    }
    for (rel, fc) in change_map {
        visible_files.insert(rel.clone());
        if !opts.dirs_only {
            if !fmap.contains_key(&rel) {
                add_ancestor_dirs(&rel, &mut dirset);
                fmap.insert(rel, fc);
            }
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
        Pcmd::new("git").args(args).current_dir(dir).output().unwrap();
    }
    fn git_out(dir: &Path, args: &[&str]) -> String {
        let o = Pcmd::new("git").args(args).current_dir(dir).output().unwrap();
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

    fn render_plain_letters(tree: &ChangeTree, root: &Path) -> String {
        let lsc = lscolors::LsColors::empty();
        TerminalRenderer {
            marks: MarkScheme::Letter,
            format: OutputFormat::Plain,
            ls_colors: &lsc,
            root: root.to_path_buf(),
        }
        .render(tree)
        .unwrap()
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
    #[test]
    fn unreadable_untracked_file_renders_read_error() {
        use std::os::unix::fs::PermissionsExt;

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
        let original_mode = std::fs::metadata(&locked).unwrap().permissions().mode();
        let _guard = ModeGuard { path: locked.clone(), mode: original_mode };
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

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
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        git(p, &["init"]);
        git(p, &["config", "user.email", "t@e.com"]);
        git(p, &["config", "user.name", "T"]);
        let tracked = p.join("tracked.txt");
        std::fs::write(&tracked, "tracked\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c0"]);

        let original_mode = std::fs::metadata(&tracked).unwrap().permissions().mode();
        let _guard = ModeGuard { path: tracked.clone(), mode: original_mode };
        std::fs::set_permissions(&tracked, std::fs::Permissions::from_mode(0o000)).unwrap();

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
        use std::os::unix::fs::PermissionsExt;

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

        let original_mode = std::fs::metadata(&tracked).unwrap().permissions().mode();
        let _guard = ModeGuard { path: tracked.clone(), mode: original_mode };
        std::fs::set_permissions(&tracked, std::fs::Permissions::from_mode(0o000)).unwrap();

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
        git(p, &["merge", "feature"]);

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
            LsColor::Fixed(_) => colored::Color::White,
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
        };
        let out = r.render(&sample_tree()).unwrap();
        assert!(out.contains("\x1b[32m"), "green present (staged mark / +N): {out:?}");
        assert!(out.contains("\x1b[31m"), "red present (deleted mark / −M): {out:?}");
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
        };
        let out = r.render(&sample_tree()).unwrap();
        assert!(!out.contains("\x1b["), "no ANSI when color off: {out:?}");
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
        };
        // gone.rs does not exist under root → style lookup must fall back, not panic.
        let _ = r.render(&sample_tree()).unwrap();
        colored::control::unset_override();
    }
}

#[cfg(test)]
mod all_files_tests {
    use super::*;
    use std::process::Command as Pcmd;

    fn git(dir: &Path, args: &[&str]) {
        Pcmd::new("git").args(args).current_dir(dir).output().unwrap();
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
