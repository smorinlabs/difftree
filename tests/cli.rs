use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

// Platform-specific import for unix permissions
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn test_nonexistent_path() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("nonexistent/path/for/testing");
    cmd.assert().failure().stderr(predicate::str::contains("is not a directory"));
    Ok(())
}

#[test]
fn test_simple_view() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("a.txt"))?;
    fs::create_dir(temp_dir.path().join("dir1"))?;
    fs::File::create(temp_dir.path().join("dir1/b.txt"))?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg(temp_dir.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("a.txt"))
        .stdout(predicate::str::contains("dir1"))
        .stdout(predicate::str::contains("b.txt"));
    Ok(())
}

#[test]
fn test_all_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join(".hidden"))?;

    let mut cmd_no_all = Command::cargo_bin("difftree")?;
    cmd_no_all.arg(temp_dir.path());
    cmd_no_all.assert().success().stdout(predicate::str::contains(".hidden").not());

    let mut cmd_with_all = Command::cargo_bin("difftree")?;
    cmd_with_all.arg("-a").arg(temp_dir.path());
    cmd_with_all.assert().success().stdout(predicate::str::contains(".hidden"));
    Ok(())
}

#[test]
fn test_depth_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::create_dir(temp_dir.path().join("dir1"))?;
    fs::File::create(temp_dir.path().join("dir1/b.txt"))?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("-L").arg("1").arg(temp_dir.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("dir1"))
        .stdout(predicate::str::contains("b.txt").not());
    Ok(())
}

#[test]
fn test_gitignore_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();

    // 1. Initialize a true git repository
    Command::new("git").arg("init").current_dir(temp_path).output()?;
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(temp_path)
        .output()?;
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(temp_path)
        .output()?;

    // 2. Create and commit the .gitignore file
    let gitignore_path = temp_path.join(".gitignore");
    fs::write(&gitignore_path, "ignored.txt\nignored_dir/\n")?;
    Command::new("git").arg("add").arg(&gitignore_path).current_dir(temp_path).output()?;
    Command::new("git")
        .arg("commit")
        .arg("-m")
        .arg("add gitignore")
        .current_dir(temp_path)
        .output()?;

    // 3. Create other files to be checked
    fs::File::create(temp_path.join("ignored.txt"))?;
    fs::File::create(temp_path.join("good.txt"))?;
    fs::create_dir(temp_path.join("ignored_dir"))?;
    fs::File::create(temp_path.join("ignored_dir/a.txt"))?;

    // 4. Run lstr, passing the temp path as an argument. This is more robust
    // than relying on `current_dir` for this specific test.
    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("-g").arg(temp_path);

    // 5. Assert that the correct files are included and excluded.
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("good.txt"))
        .stdout(predicate::str::contains("ignored.txt").not())
        .stdout(predicate::str::contains("ignored_dir").not());

    Ok(())
}

#[test]
#[cfg(unix)]
fn test_permissions_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let file_path = temp_dir.path().join("test_file.txt");
    fs::File::create(&file_path)?;

    let perms = fs::Permissions::from_mode(0o550);
    fs::set_permissions(&file_path, perms)?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("-p").arg(temp_dir.path());
    cmd.assert().success().stdout(predicate::str::contains("-r-xr-x---"));

    Ok(())
}

#[test]
fn test_git_status_flag() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();

    Command::new("git").arg("init").current_dir(temp_path).output()?;
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(temp_path)
        .output()?;
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(temp_path)
        .output()?;

    fs::write(temp_path.join("committed.txt"), "initial content")?;
    Command::new("git").args(["add", "committed.txt"]).current_dir(temp_path).output()?;
    Command::new("git").args(["commit", "-m", "initial commit"]).current_dir(temp_path).output()?;

    fs::write(temp_path.join("committed.txt"), "modified content")?;
    fs::write(temp_path.join("staged.txt"), "staged")?;
    Command::new("git").args(["add", "staged.txt"]).current_dir(temp_path).output()?;
    fs::write(temp_path.join("untracked.txt"), "untracked")?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("-G").arg("-a").arg(temp_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::is_match(r"M\s+.*committed\.txt").unwrap())
        .stdout(predicate::str::is_match(r"A\s+.*staged\.txt").unwrap())
        .stdout(predicate::str::is_match(r"\?\s+.*untracked\.txt").unwrap());

    Ok(())
}

#[test]
fn test_sort_by_name() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("zebra.txt"))?;
    fs::File::create(temp_dir.path().join("apple.txt"))?;
    fs::File::create(temp_dir.path().join("banana.txt"))?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--sort").arg("name").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    // Files should appear in alphabetical order
    let apple_pos = stdout.find("apple.txt").unwrap();
    let banana_pos = stdout.find("banana.txt").unwrap();
    let zebra_pos = stdout.find("zebra.txt").unwrap();

    assert!(apple_pos < banana_pos);
    assert!(banana_pos < zebra_pos);

    Ok(())
}

#[test]
fn test_dirs_first_sorting() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("aaa_file.txt"))?;
    fs::create_dir(temp_dir.path().join("zzz_dir"))?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--dirs-first").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    // Directory should appear before file, despite alphabetical order
    let dir_pos = stdout.find("zzz_dir").unwrap();
    let file_pos = stdout.find("aaa_file.txt").unwrap();

    assert!(dir_pos < file_pos);

    Ok(())
}

#[test]
fn test_natural_sorting() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("file1.txt"))?;
    fs::File::create(temp_dir.path().join("file10.txt"))?;
    fs::File::create(temp_dir.path().join("file2.txt"))?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--natural-sort").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    // With natural sorting: file1 < file2 < file10
    let file1_pos = stdout.find("file1.txt").unwrap();
    let file2_pos = stdout.find("file2.txt").unwrap();
    let file10_pos = stdout.find("file10.txt").unwrap();

    assert!(file1_pos < file2_pos);
    assert!(file2_pos < file10_pos);

    Ok(())
}

#[test]
fn test_reverse_sorting() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("apple.txt"))?;
    fs::File::create(temp_dir.path().join("zebra.txt"))?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--reverse").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    // With reverse sorting: zebra should come before apple
    let apple_pos = stdout.find("apple.txt").unwrap();
    let zebra_pos = stdout.find("zebra.txt").unwrap();

    assert!(zebra_pos < apple_pos);

    Ok(())
}

#[test]
fn test_case_sensitive_sorting() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("Apple.txt"))?;
    fs::File::create(temp_dir.path().join("banana.txt"))?;

    // Test case-sensitive (Apple should come before banana in ASCII)
    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--case-sensitive").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    let apple_pos = stdout.find("Apple.txt").unwrap();
    let banana_pos = stdout.find("banana.txt").unwrap();

    // In case-sensitive ASCII order: "Apple" < "banana" (uppercase < lowercase)
    assert!(apple_pos < banana_pos);

    Ok(())
}

#[test]
fn test_sort_by_extension() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    fs::File::create(temp_dir.path().join("file.zzz"))?;
    fs::File::create(temp_dir.path().join("file.aaa"))?;
    fs::File::create(temp_dir.path().join("file.bbb"))?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--sort").arg("extension").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    // Files should be sorted by extension: .aaa < .bbb < .zzz
    let aaa_pos = stdout.find("file.aaa").unwrap();
    let bbb_pos = stdout.find("file.bbb").unwrap();
    let zzz_pos = stdout.find("file.zzz").unwrap();

    assert!(aaa_pos < bbb_pos);
    assert!(bbb_pos < zzz_pos);

    Ok(())
}

#[test]
fn test_default_sort_order() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;

    // Create files with explicit writes and different names to avoid conflicts
    let file1_path = temp_dir.path().join("0num.txt");
    let file_a_path = temp_dir.path().join("Upper.txt");
    let file_a_lower_path = temp_dir.path().join("lower.txt");

    fs::write(&file1_path, "1")?;
    fs::write(&file_a_path, "A")?;
    fs::write(&file_a_lower_path, "a")?;

    // Verify files exist
    assert!(file1_path.exists(), "0num.txt was not created");
    assert!(file_a_path.exists(), "Upper.txt was not created");
    assert!(file_a_lower_path.exists(), "lower.txt was not created");

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--case-sensitive").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    // Check if files are at least present
    assert!(stdout.contains("0num.txt"), "0num.txt missing from output");
    assert!(stdout.contains("Upper.txt"), "Upper.txt missing from output");
    assert!(stdout.contains("lower.txt"), "lower.txt missing from output");

    // With default order: numbers < uppercase < lowercase
    let file1_pos = stdout.find("0num.txt").expect("0num.txt not found in output");
    let file_a_pos = stdout.find("Upper.txt").expect("Upper.txt not found in output");
    let file_a_lower_pos = stdout.find("lower.txt").expect("lower.txt not found in output");

    assert!(file1_pos < file_a_pos);
    assert!(file_a_pos < file_a_lower_pos);

    Ok(())
}

#[test]
fn test_dotfiles_first_sorting() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;

    // Create files and folders with explicit writes/creates
    fs::write(temp_dir.path().join("regular.txt"), "regular")?;
    fs::write(temp_dir.path().join(".hidden.txt"), "hidden")?;
    fs::create_dir(temp_dir.path().join("folder"))?;
    fs::create_dir(temp_dir.path().join(".dotfolder"))?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--dotfiles-first").arg("-a").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    // Order should be: .dotfolder -> folder -> .hidden.txt -> regular.txt
    // With proper tree connectors: ├── for first 3 items, └── for last item
    let dotfolder_line_pos = stdout.find("├── .dotfolder").expect(".dotfolder line not found");
    let folder_line_pos = stdout.find("├── folder").expect("folder line not found");
    let hidden_line_pos = stdout.find("├── .hidden.txt").expect(".hidden.txt line not found");
    let regular_line_pos = stdout.find("└── regular.txt").expect("regular.txt line not found");

    assert!(dotfolder_line_pos < folder_line_pos);
    assert!(folder_line_pos < hidden_line_pos);
    assert!(hidden_line_pos < regular_line_pos);

    Ok(())
}

#[test]
fn test_tree_structure_display() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;

    // Create the exact structure from issue #36:
    // .
    // └── t1
    //     ├── t2
    //     │   ├── hello.md
    //     │   └── t3
    //     └── tmp.txt

    // Create t1 directory
    fs::create_dir(temp_dir.path().join("t1"))?;

    // Create t2 subdirectory
    fs::create_dir(temp_dir.path().join("t1/t2"))?;

    // Create files and subdirectories
    fs::write(temp_dir.path().join("t1/t2/hello.md"), "# Hello")?;
    fs::create_dir(temp_dir.path().join("t1/t2/t3"))?;
    fs::write(temp_dir.path().join("t1/tmp.txt"), "temporary content")?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    // Expected tree structure with proper connectors:
    // t1 should use └── (last/only item in root)
    // t2 should use ├── (not last in t1)
    // tmp.txt should use └── (last in t1)
    // hello.md should use ├── (not last in t2)
    // t3 should use └── (last in t2)

    // Check that we have proper tree connectors, not all └──
    assert!(stdout.contains("└── t1"), "t1 should use └── connector");
    assert!(stdout.contains("├── t2"), "t2 should use ├── connector (not last in parent)");
    assert!(stdout.contains("└── tmp.txt"), "tmp.txt should use └── connector (last in parent)");
    assert!(
        stdout.contains("├── hello.md"),
        "hello.md should use ├── connector (not last in parent)"
    );
    assert!(stdout.contains("└── t3"), "t3 should use └── connector (last in parent)");

    // Verify we have vertical connectors for proper tree visualization
    assert!(stdout.contains("│"), "Should contain vertical tree connectors");

    Ok(())
}

#[test]
fn test_tree_structure_with_dirs_first() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;

    // Create a structure to test --dirs-first with tree connectors
    fs::create_dir(temp_dir.path().join("dir1"))?;
    fs::create_dir(temp_dir.path().join("dir2"))?;
    fs::write(temp_dir.path().join("file1.txt"), "content1")?;
    fs::write(temp_dir.path().join("file2.txt"), "content2")?;

    // Add some nested content
    fs::write(temp_dir.path().join("dir1/nested.txt"), "nested content")?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--dirs-first").arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    // With --dirs-first, directories should come first
    // dir1 should use ├── (not last directory)
    // dir2 should use ├── (not last directory, files come after)
    // file1.txt should use ├── (not last file)
    // file2.txt should use └── (last file)

    assert!(stdout.contains("├── dir1"), "dir1 should use ├── connector");
    assert!(stdout.contains("├── dir2"), "dir2 should use ├── connector");
    assert!(stdout.contains("├── file1.txt"), "file1.txt should use ├── connector");
    assert!(stdout.contains("└── file2.txt"), "file2.txt should use └── connector (last)");

    Ok(())
}

#[test]
fn test_single_file_tree() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;

    // Single file should use └──
    fs::write(temp_dir.path().join("single.txt"), "content")?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    assert!(stdout.contains("└── single.txt"), "single file should use └── connector");

    Ok(())
}

#[test]
fn test_deep_nested_tree() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;

    // Create a deeper nesting structure: a/b/c/d/file.txt
    fs::create_dir(temp_dir.path().join("a"))?;
    fs::create_dir(temp_dir.path().join("a/b"))?;
    fs::create_dir(temp_dir.path().join("a/b/c"))?;
    fs::create_dir(temp_dir.path().join("a/b/c/d"))?;
    fs::write(temp_dir.path().join("a/b/c/d/deep.txt"), "deep content")?;

    // Add sibling to 'a' to test vertical connectors
    fs::create_dir(temp_dir.path().join("sibling"))?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg(temp_dir.path());

    let output = cmd.output()?;
    let stdout = String::from_utf8(output.stdout)?;

    // Should have proper vertical connectors for deep nesting
    assert!(stdout.contains("├── a"), "a should use ├── (has sibling)");
    assert!(stdout.contains("└── sibling"), "sibling should use └── (last)");
    assert!(stdout.contains("│"), "Should contain vertical connectors for deep nesting");

    Ok(())
}

#[test]
fn test_json_schema_version_for_staged_change() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();
    Command::new("git").arg("init").current_dir(temp_path).output()?;
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(temp_path)
        .output()?;
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(temp_path)
        .output()?;
    fs::write(temp_path.join("changed.txt"), "hello")?;
    Command::new("git").args(["add", "changed.txt"]).current_dir(temp_path).output()?;

    let output = Command::cargo_bin("difftree")?.arg("--json").arg(temp_path).output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("\"schema_version\": \"difftree.v1\""));
    assert!(stdout.contains("changed.txt"));
    Ok(())
}

#[test]
fn test_default_fallback_wording_when_only_unstaged() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();
    Command::new("git").arg("init").current_dir(temp_path).output()?;
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(temp_path)
        .output()?;
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(temp_path)
        .output()?;
    fs::write(temp_path.join("tracked.txt"), "one")?;
    Command::new("git").args(["add", "tracked.txt"]).current_dir(temp_path).output()?;
    Command::new("git").args(["commit", "-m", "initial"]).current_dir(temp_path).output()?;
    fs::write(temp_path.join("tracked.txt"), "two")?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg(temp_path);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No staged changes — showing unstaged changes"))
        .stdout(predicate::str::contains("tracked.txt"));
    Ok(())
}

#[test]
fn test_mark_scheme_letter() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();
    Command::new("git").arg("init").current_dir(temp_path).output()?;
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(temp_path)
        .output()?;
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(temp_path)
        .output()?;
    fs::write(temp_path.join("new.txt"), "hello")?;
    Command::new("git").args(["add", "new.txt"]).current_dir(temp_path).output()?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--marks").arg("letter").arg(temp_path);
    cmd.assert().success().stdout(predicate::str::contains("S new.txt"));
    Ok(())
}

#[test]
fn test_uncommitted_shows_staged_and_unstaged() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::write(p.join("base.txt"), "one")?;
    Command::new("git").args(["add", "base.txt"]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "init"]).current_dir(p).output()?;
    // one staged new file + one unstaged modification
    fs::write(p.join("staged.txt"), "s")?;
    Command::new("git").args(["add", "staged.txt"]).current_dir(p).output()?;
    fs::write(p.join("base.txt"), "two")?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--uncommitted").arg(p);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("staged.txt"))
        .stdout(predicate::str::contains("base.txt"));
    Ok(())
}

#[test]
fn test_staged_flag_does_not_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::write(p.join("base.txt"), "one")?;
    Command::new("git").args(["add", "base.txt"]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "init"]).current_dir(p).output()?;
    fs::write(p.join("base.txt"), "two")?; // only unstaged

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--staged").arg(p);
    cmd.assert().success().stdout(predicate::str::contains("No staged changes").not());
    Ok(())
}

#[test]
fn test_json_includes_view_field() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::write(p.join("changed.txt"), "hi")?;
    Command::new("git").args(["add", "changed.txt"]).current_dir(p).output()?;

    let output = Command::cargo_bin("difftree")?.arg("--json").arg(p).output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("\"view\": \"blast-radius\""));
    Ok(())
}

#[test]
fn test_all_files_view_shows_unchanged_files() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::create_dir(p.join("src"))?;
    fs::create_dir(p.join("docs"))?;
    fs::write(p.join("src/changed.rs"), "a")?;
    fs::write(p.join("docs/readme.md"), "b")?;
    Command::new("git").args(["add", "."]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "init"]).current_dir(p).output()?;
    fs::write(p.join("src/changed.rs"), "a2")?;
    Command::new("git").args(["add", "src/changed.rs"]).current_dir(p).output()?;

    for flag in ["--all", "--tree"] {
        let mut cmd = Command::cargo_bin("difftree")?;
        cmd.arg(flag).arg(p);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("changed.rs"))
            .stdout(predicate::str::contains("readme.md"));
    }
    Ok(())
}

#[test]
fn test_all_files_json_marks_unchanged_clean() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::write(p.join("unchanged.txt"), "x")?;
    fs::write(p.join("staged.txt"), "y")?;
    Command::new("git").args(["add", "unchanged.txt"]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "init"]).current_dir(p).output()?;
    fs::write(p.join("staged.txt"), "y")?;
    Command::new("git").args(["add", "staged.txt"]).current_dir(p).output()?;

    let output = Command::cargo_bin("difftree")?.arg("--all").arg("--json").arg(p).output()?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("\"view\": \"all-files\""));
    assert!(stdout.contains("unchanged.txt"));
    assert!(stdout.contains("\"Clean\""));
    Ok(())
}

#[test]
fn test_clean_repo_no_fallback_banner() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::write(p.join("f.txt"), "x")?;
    Command::new("git").args(["add", "f.txt"]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "init"]).current_dir(p).output()?;

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg(p);
    cmd.assert().success().stdout(predicate::str::contains("No staged changes").not());
    Ok(())
}

#[test]
fn test_json_outside_git_repo_errors() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?; // not a git repo
    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--json").arg(temp_dir.path());
    cmd.assert().failure().stderr(predicate::str::contains("requires a git repository"));
    Ok(())
}

#[test]
fn test_staged_outside_git_repo_errors() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?; // not a git repo
    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--staged").arg(temp_dir.path());
    cmd.assert().failure().stderr(predicate::str::contains("requires a git repository"));
    Ok(())
}

#[test]
fn test_range_excludes_untracked() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::write(p.join("a.txt"), "1")?;
    Command::new("git").args(["add", "a.txt"]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "c1"]).current_dir(p).output()?;
    fs::write(p.join("b.txt"), "2")?;
    Command::new("git").args(["add", "b.txt"]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "c2"]).current_dir(p).output()?;
    fs::write(p.join("untracked_xyz.txt"), "u")?; // present but unrelated to the range

    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--range").arg("HEAD~1..HEAD").arg(p);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("b.txt"))
        .stdout(predicate::str::contains("untracked_xyz.txt").not());
    Ok(())
}

#[test]
fn test_all_files_depth_filter_no_spurious_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let p = temp_dir.path();
    Command::new("git").arg("init").current_dir(p).output()?;
    Command::new("git").args(["config", "user.email", "t@e.com"]).current_dir(p).output()?;
    Command::new("git").args(["config", "user.name", "T"]).current_dir(p).output()?;
    fs::create_dir(p.join("src"))?;
    fs::write(p.join("src/deep.rs"), "a")?;
    Command::new("git").args(["add", "."]).current_dir(p).output()?;
    Command::new("git").args(["commit", "-m", "init"]).current_dir(p).output()?;
    // Stage a change that sits BELOW the -L 1 cutoff.
    fs::write(p.join("src/deep.rs"), "a2")?;
    Command::new("git").args(["add", "src/deep.rs"]).current_dir(p).output()?;

    // A staged change exists, so the all-files view must NOT claim "No staged changes",
    // even though -L 1 hides the changed file from the rendered tree.
    let mut cmd = Command::cargo_bin("difftree")?;
    cmd.arg("--all").arg("-L").arg("1").arg(p);
    cmd.assert().success().stdout(predicate::str::contains("No staged changes").not());
    Ok(())
}
