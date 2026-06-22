//! Core difftree library: serializable model, git-backed collection, and renderers.

use git2::{DiffOptions, Repository};
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
    Deleted,
    Ignored,
    Clean,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TreeNode {
    pub name: String,
    pub path: String,
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
pub struct TerminalRenderer {
    pub marks: MarkScheme,
    pub format: OutputFormat,
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

impl Renderer for TerminalRenderer {
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
            "\n{} dirs touched · {} files changed · +{} −{}\n",
            tree.summary.dirs_touched,
            tree.summary.files_changed,
            tree.summary.churn.added,
            tree.summary.churn.deleted
        ));
        Ok(out)
    }
}
impl TerminalRenderer {
    fn node(&self, out: &mut String, n: &TreeNode, prefix: &str, last: bool) {
        let conn = if last { "└──" } else { "├──" };
        let mark = mark(n, self.marks);
        let metric = if n.kind == NodeKind::Directory {
            format!(
                " ({} files, +{} −{})",
                n.rollup.files_changed, n.rollup.churn.added, n.rollup.churn.deleted
            )
        } else {
            format!(" +{} −{}", n.churn.added, n.churn.deleted)
        };
        out.push_str(&format!("{prefix}{conn} {mark} {}{metric}\n", n.name));
        let next = format!("{}{}", prefix, if last { "    " } else { "│   " });
        for (idx, c) in n.children.iter().enumerate() {
            self.node(out, c, &next, idx + 1 == n.children.len());
        }
    }
}
fn mark(n: &TreeNode, s: MarkScheme) -> &'static str {
    match s {
        MarkScheme::Symbol => match n.status {
            ChangeStatus::Staged => "●",
            ChangeStatus::Unstaged => "○",
            ChangeStatus::Both => "◐",
            ChangeStatus::Untracked => "?",
            ChangeStatus::Renamed => "↻",
            ChangeStatus::Deleted => "×",
            ChangeStatus::Ignored => "!",
            ChangeStatus::Clean => " ",
        },
        MarkScheme::Letter => match n.status {
            ChangeStatus::Staged => "S",
            ChangeStatus::Unstaged => "M",
            ChangeStatus::Both => "B",
            ChangeStatus::Untracked => "?",
            ChangeStatus::Renamed => "R",
            ChangeStatus::Deleted => "D",
            ChangeStatus::Ignored => "I",
            ChangeStatus::Clean => " ",
        },
        MarkScheme::Xy => match n.status {
            ChangeStatus::Staged => "M ",
            ChangeStatus::Unstaged => " M",
            ChangeStatus::Both => "MM",
            ChangeStatus::Untracked => "??",
            ChangeStatus::Renamed => "R ",
            ChangeStatus::Deleted => "D ",
            ChangeStatus::Ignored => "!!",
            ChangeStatus::Clean => "  ",
        },
    }
}

#[derive(Debug, Clone)]
struct FileChange {
    path: PathBuf,
    status: ChangeStatus,
    churn: Churn,
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
    if include_untracked {
        add_untracked(&repo, &mut files)?;
    }
    files.retain(|f| scope_rel.as_os_str().is_empty() || f.path.starts_with(scope_rel));
    let root_name = if scope_rel.as_os_str().is_empty() {
        workdir.file_name().and_then(|s| s.to_str()).unwrap_or(".").to_string()
    } else {
        scope_rel.display().to_string()
    };
    Ok(Some(build_tree(root_name, mode, View::BlastRadius, files, None)))
}

pub fn collect_default_with_fallback(start: &Path) -> anyhow::Result<Option<ChangeTree>> {
    let mut staged = collect_changes(start, ComparisonMode::Staged, true)?;
    if staged.as_ref().is_some_and(|t| t.summary.files_changed > 0) {
        return Ok(staged);
    }
    let mut unstaged = collect_changes(start, ComparisonMode::Unstaged, true)?;
    if let Some(t) = &mut unstaged {
        t.fallback = Some("No staged changes — showing unstaged blast radius".to_string());
    }
    if unstaged.is_some() {
        staged = unstaged;
    }
    Ok(staged)
}

fn diff_files(repo: &Repository, mode: &ComparisonMode) -> anyhow::Result<Vec<FileChange>> {
    let mut opts = DiffOptions::new();
    opts.include_untracked(false).recurse_untracked_dirs(true);
    let diff = match mode {
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
    };
    let mut out = Vec::new();
    diff.foreach(
        &mut |d, _| {
            let path = d.new_file().path().or_else(|| d.old_file().path()).unwrap_or(Path::new(""));
            let status = match d.status() {
                git2::Delta::Deleted => ChangeStatus::Deleted,
                git2::Delta::Renamed => ChangeStatus::Renamed,
                _ => match mode {
                    ComparisonMode::Unstaged => ChangeStatus::Unstaged,
                    _ => ChangeStatus::Staged,
                },
            };
            out.push(FileChange { path: path.to_path_buf(), status, churn: Churn::default() });
            true
        },
        None,
        None,
        None,
    )?;
    Ok(out)
}
fn add_untracked(repo: &Repository, files: &mut Vec<FileChange>) -> anyhow::Result<()> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    for e in repo.statuses(Some(&mut opts))?.iter() {
        if e.status().is_wt_new() {
            if let Some(p) = e.path() {
                files.push(FileChange {
                    path: PathBuf::from(p),
                    status: ChangeStatus::Untracked,
                    churn: Churn::default(),
                });
            }
        }
    }
    Ok(())
}
fn build_tree(
    root_name: String,
    mode: ComparisonMode,
    view: View,
    files: Vec<FileChange>,
    fallback: Option<String>,
) -> ChangeTree {
    let mut dirset = BTreeSet::new();
    let mut fmap = BTreeMap::new();
    for f in files {
        for a in f.path.ancestors().skip(1) {
            if !a.as_os_str().is_empty() {
                dirset.insert(a.to_path_buf());
            }
        }
        fmap.insert(f.path.clone(), f);
    }
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
                name: path.file_name().unwrap().to_string_lossy().to_string(),
                path: path.display().to_string(),
                kind: NodeKind::File,
                status: f.status.clone(),
                churn: f.churn.clone(),
                rollup: Rollup { dirs_touched: 0, files_changed: 1, churn: f.churn.clone() },
                children: vec![],
            }
        } else {
            let mut ch: Vec<_> =
                child_paths(path, dirs, files).iter().map(|p| mk(p, dirs, files)).collect();
            ch.sort_by(|a, b| a.name.cmp(&b.name));
            let mut r = Rollup::default();
            for c in &ch {
                if c.kind == NodeKind::Directory {
                    r.dirs_touched += 1 + c.rollup.dirs_touched;
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
            summary.dirs_touched += 1 + c.rollup.dirs_touched;
        }
        summary.files_changed += c.rollup.files_changed;
        summary.churn.added += c.rollup.churn.added;
        summary.churn.deleted += c.rollup.churn.deleted;
    }
    let root = TreeNode {
        name: root_name,
        path: "".into(),
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
