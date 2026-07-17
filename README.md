# difftree

[![Crates.io](https://img.shields.io/crates/v/difftree.svg)](https://crates.io/crates/difftree)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A fast, minimalist, git-aware directory tree viewer, written in Rust.

> difftree is a fork of [lstr](https://github.com/bgreenwell/lstr) by Brandon
> Greenwell that now ships its own git-aware comparison features: PR-style diff
> trees (`--pr`), ref and range comparisons (`--against <ref>`, `--range <a..b>`),
> staged/unstaged/uncommitted views, an all-files view (`--all`), status marks
> (`--marks`), per-file churn display, and `--json` structured output. See
> [Credits / Attribution](#credits--attribution) and `NOTICE`.

![](assets/lstr-demo.gif)

*An interactive overview of a project's structure (demo inherited from lstr).*

## Philosophy

  - **Minimalist:** Provides essential features without the bloat. The core experience is clean and uncluttered.
  - **Interactive:** An optional TUI mode for fluid, keyboard-driven exploration.

## Features

  - **Git-aware comparison views:**
      - `--pr`: PR-style diff for the current branch — everything changed since it diverged from the base (merge-base → HEAD), **including uncommitted changes by default**; add `--committed` to show committed changes only. Base is auto-detected (`origin` default → `main` → `master`); override with `--pr=<ref>` (or `--pr --pr-base <ref>` — `--pr-base` requires `--pr`).
      - `--against <ref>` and `--range <a..b>` for explicit ref and range comparisons.
      - `--staged` (alias `--cached`), `--unstaged`, and `--uncommitted` working-tree comparisons.
      - All-files view (`--all`, alias `--tree`): the complete directory tree with change marks overlaid.
      - Status marks (`--marks=symbol|letter|xy`) and per-file churn display (`+N −M` line counts).
      - `--json` structured output (schema `difftree.v2`).
  - **Classic and interactive modes:** Use `difftree` for a classic `tree`-like view, or launch `difftree interactive` for a fully interactive TUI.
  - **Theme-aware coloring:** Respects your system's `LS_COLORS` environment variable for fully customizable file and directory colors.
  - **Rich information display (optional):**
      - Display file-specific icons with `--icons` (requires a Nerd Font).
      - Show file permissions with `-p`.
      - Show file sizes with `-s`.
      - **Git Integration:** Show file statuses (`Modified`, `New`, `Untracked`, etc.) directly in the tree with the `-G` flag.
  - **Smart filtering:**
      - Respects your `.gitignore` files with the `-g` flag.
      - Control recursion depth (`-L`) or show only directories (`-d`).

## Installation

### From crates.io

`difftree` is published on [crates.io](https://crates.io/crates/difftree) (v0.3.x):

```bash
cargo install difftree
```

### From source

You need the Rust toolchain installed on your system to build `difftree`.

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/smorinlabs/difftree.git
    cd difftree
    ```
2.  **Build and install using Cargo:**
    ```bash
    cargo install --path .
    ```

### GitHub Action

The companion action [smorinlabs/difftree-action](https://github.com/smorinlabs/difftree-action)
posts a diff-tree of a pull request's changes (rendered by `difftree --pr`) as a
self-updating PR comment.

To wire it into a repo with an agent, this repo ships a small
[`difftree-setup`](.claude/skills/difftree-setup/SKILL.md) pointer skill
(auto-discovered by Claude Code and Codex; see
[docs/skills/difftree-setup.md](docs/skills/difftree-setup.md)) that routes you
to the canonical setup skill living in the difftree-action repo.

## Usage

```bash
difftree [OPTIONS] [PATH]
difftree interactive [OPTIONS] [PATH]
```

Note that `PATH` defaults to the current directory (`.`) if not specified.

| Option                 | Description                                                                 |
| :--------------------- | :-------------------------------------------------------------------------- |
| `-a`, `--show-all`     | List all files and directories, including hidden ones. (Interactive mode uses `-a`, `--all`.) |
| `--all` (alias `--tree`)| Render the all-files view: the complete tree with git change marks overlaid. |
| `--color <WHEN>`       | Specify when to use color output (`always`, `auto`, `never`).               |
| `-d`, `--dirs-only`    | List directories only, ignoring all files.                                  |
| `-g`, `--gitignore`    | Respect `.gitignore` and other standard ignore files.                       |
| `-G`, `--git-status`   | Show git status for files and directories.                                  |
| `--icons`              | Display file-specific icons; requires a [Nerd Font](https://www.nerdfonts.com/). |
| `--hyperlinks`         | Render file paths as clickable hyperlinks (classic mode only)               |
| `-L`, `--level <LEVEL>`| Maximum depth to descend.                                                   |
| `-p`, `--permissions`  | Display file permissions (Unix-like systems only).                          |
| `-s`, `--size`         | Display the size of files.                                                  |
| `--sort <TYPE>`        | Sort entries by the specified criteria (`name`, `size`, `modified`, `extension`). |
| `--dirs-first`         | Sort directories before files.                                              |
| `--case-sensitive`     | Use case-sensitive sorting.                                                 |
| `--natural-sort`       | Use natural/version sorting (e.g., file1 < file10).                        |
| `-r`, `--reverse`      | Reverse the sort order.                                                     |
| `--dotfiles-first`     | Sort dotfiles and dotfolders first (dotfolders → folders → dotfiles → files). |
| `--expand-level <LEVEL>`| **Interactive mode only:** Initial depth to expand the interactive tree.   |

-----

## Interactive mode

Launch the TUI with `difftree interactive [OPTIONS] [PATH]`.

### Keyboard controls

| Key(s)  | Action                                                                                                                                      |
| :------ | :------------------------------------------------------------------------------------------------------------------------------------------ |
| `↑` / `k` | Move selection up. |
| `↓` / `j` | Move selection down. |
| `Enter` | **Context-aware action:**\<br\>- If on a file: Open it in the default editor (`$EDITOR`).\<br\>- If on a directory: Toggle expand/collapse. |
| `q` / `Esc` | Quit the application normally. |
| `Ctrl`+`s` | **Shell integration:** Quits and prints the selected path to stdout. |

## Examples

**1. List the contents of the current directory**

```bash
difftree
```

**2. Explore a project interactively, ignoring gitignored files**

```bash
difftree interactive -g --icons
```

**3. Display a directory with file sizes and permissions (classic view)**

```bash
difftree -sp
```

**4. See the git status of all files in a project**

```bash
difftree -aG
```

**5. Get a tree with clickable file links (in a supported terminal)**

```bash
difftree --hyperlinks
```

**6. Start an interactive session with all data displayed**

```bash
difftree interactive -gG --icons -s -p
```

**7. Sort files naturally with directories first**

```bash
difftree --dirs-first --natural-sort
```

**8. Sort by file size in descending order**

```bash
difftree --sort size --reverse
```

**9. Sort by extension with case-sensitive ordering**

```bash
difftree --sort extension --case-sensitive
```

**10. Sort with dotfiles first and directories first**

```bash
difftree --dotfiles-first --dirs-first -a
```

## Piping and shell interaction

The classic `view` mode is designed to work well with other command-line tools via pipes (`|`).

### Interactive fuzzy finding with `fzf`

This is a powerful way to instantly find any file in a large project.

```bash
difftree -a -g --icons | fzf
```

`fzf` will take the tree from `difftree` and provide an interactive search prompt to filter it.

### Paging large trees with `less` or `bat`

If a directory is too large to fit on one screen, pipe the output to a *pager*.

```bash
# Using less (the -R flag preserves color)
difftree -L 10 | less -R

# Using bat (a modern pager that understands colors)
difftree --icons | bat
```

### Changing directories with `difftree`

You can use `difftree` as a visual `cd` command. Add the following function to your shell's startup file (e.g., `~/.bashrc`, `~/.zshrc`):

```bash
# A function to visually change directories with difftree
lcd() {
    # Run difftree and capture the selected path into a variable.
    # The TUI will draw on stderr, and the final path will be on stdout.
    local selected_dir
    selected_dir="$(difftree interactive -g --icons)"

    # If the user selected a path (and didn't just quit), `cd` into it.
    # Check if the selection is a directory.
    if [[ -n "$selected_dir" && -d "$selected_dir" ]]; then
        cd "$selected_dir"
    fi
}
```

After adding this and starting a new shell session (or running `source ~/.bashrc`), you can simply run:

```bash
lcd
```

This will launch the `difftree` interactive UI. Navigate to the directory you want, press `Ctrl+s`, and your shell's current directory will instantly change.

## Color customization

`difftree` respects your terminal's color theme by default. It reads the `LS_COLORS` environment variable to colorize files and directories according to your system's configuration. This is the same variable used by GNU `ls` and other modern command-line tools.

### Linux

On most Linux distributions, this variable is already set. You can customize it by modifying your shell's startup file.

### macOS

macOS does not set the `LS_COLORS` variable by default. To enable this feature, you can install `coreutils`:

```bash
brew install coreutils
```

Then, add the following line to your shell's startup file (e.g., `~/.zshrc` or `~/.bash_profile`):

```bash
# Use gdircolors from the newly installed coreutils
eval "$(gdircolors)"
```

### Windows

Windows does not use the `LS_COLORS` variable natively, but you can set it manually to enable color support in modern terminals like Windows Terminal.

First, copy a standard `LS_COLORS` string, such as this one:
`rs=0:di=01;34:ln=01;36:ex=01;32:*.zip=01;31:*.png=01;35:`. This string defines colors for various file types:

* **Directories:** Displayed in **bold blue**.
* **Executable files:** Displayed in **bold green** (e.g., `.sh` scripts).
* **Symbolic links:** Displayed in **bold cyan**.
* **Archives:** Displayed in **bold red** (e.g., `.zip`, `.tar.gz`).
* **Image files:** Displayed in **bold magenta** (e.g., `.png`, `.jpg`).
* **Other files:** Displayed in the terminal's default text color.

To set it for your current **PowerShell** session, run:

```powershell
$env:LS_COLORS="rs=0:di=01;34:ln=01;36:ex=01;32:*.zip=01;31:*.png=01;35:"
```

To set it for your current **Command Prompt** (cmd) session, run:

```cmd
set LS_COLORS=rs=0:di=01;34:ln=01;36:ex=01;32:*.zip=01;31:*.png=01;35:
```

To make the setting permanent, you can add the command to your PowerShell profile or set it in the system's "Environment Variables" dialog.

After setting the variable and starting a new shell session, `difftree` will automatically display your configured colors.

## Inspiration

The philosophy and functionality of difftree (via lstr) are heavily inspired by the excellent C-based [tree](https://github.com/Old-Man-Programmer/tree) command line program. This project is an attempt to recreate that classic utility in modern, safe Rust.

## Credits / Attribution

difftree is a fork of **[lstr](https://github.com/bgreenwell/lstr)** by **Brandon Greenwell**, used under the MIT License. It was seeded from lstr version 0.2.1 (commit `7e522189e9acc0a8a50d08b9730d1f5c96c7f014`).

Enormous thanks to Brandon Greenwell and the lstr contributors for the original work. The original copyright and MIT license terms are preserved in [`LICENSE`](LICENSE), and provenance details are recorded in [`NOTICE`](NOTICE).

## License

This project is licensed under the terms of the [MIT License](LICENSE).

## Git-aware comparison views

difftree's git-aware blast-radius surface (introduced in v0.2 per `docs/PRD/difftree-prd-v0.2.md`, refined in v0.3):

- Bare `difftree` in a git repository shows staged blast radius and falls back with `No staged changes — showing unstaged blast radius` when staged changes are empty.
- Comparison modes: `--staged` (alias `--cached`), `--unstaged`, `--uncommitted` (staged + unstaged + untracked vs `HEAD`), `--range <A..B>`, and `--against <ref>`.
- All-files view (`--all`, alias `--tree`): renders the complete directory tree with git change marks overlaid; unchanged files are shown as `Clean`.
- `--pr` shows the PR-style diff for the current branch: everything changed since it diverged from the base (the merge-base). The base auto-detects (`origin` default → `main` → `master`, preferring the `origin/<name>` remote ref); pass `--pr=<ref>` (or `--pr --pr-base <ref>`; `--pr-base` requires `--pr`) to override. Positional paths remain path scopes, so `difftree --pr src` means "show the PR diff under `src`." Default endpoint is the working tree (commits + staged + unstaged + untracked); add `--committed` to narrow to committed branch commits only (`merge-base → HEAD`).
- Comparison views (`--pr`, `--staged`, `--all`, …) are colorized when color is enabled
  (status marks by git state, `+N` green / `−M` red churn, and filenames via `LS_COLORS`).
  Honors `--color=<when>`, `--force-color`/`-C`, `NO_COLOR`, and auto-disables when piped.
- Every comparison view renders a context header above the tree. For example, `--pr`
  renders `PR: origin/main...feature · working tree` (or `· committed` with
  `--committed`), while `--against <ref>`, `--range <A..B>`, `--staged`,
  `--unstaged`, and `--uncommitted` render mode-specific headers.
- The footer keeps the existing directory/churn summary and adds a GitHub-style
  change-kind count. A single-kind set collapses to `2 files modified`; mixed sets
  render inline, such as `3 files changed (1 added · 1 modified · 1 deleted)`.
- `--plain` (alias `--no-git`) preserves classic tree behavior.
- `--json` is available on every rendering command path with `schema_version:
  "difftree.v2"`. Git-aware modes and views (`difftree --json`, `--all --json`,
  `--pr --json`, and related comparison flags) emit the `ChangeTree` model.
  Comparison nodes expose structural node type as `node_kind`; changed file nodes
  expose their delta kind as `kind` (`added`, `modified`, `deleted`, `renamed`,
  `copied`, `typechanged`, `conflicted`, or `unreadable`). Classic/plain tree
  paths (`--plain --json`, non-git fallbacks, `-G --json`, and
  `interactive --json`) emit a plain-tree JSON model. Explicit git comparisons
  such as `--staged --json` and `--pr --json` still require a git repository.
- `--marks=symbol|letter|xy` controls status marks. (`--heat` is accepted for the v1 heat-component grammar but is not yet wired to rendering.)

See `docs/specs/difftree-decisions-v0.2.md` for the locked flag table and JSON contract.
