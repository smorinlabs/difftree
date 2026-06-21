# Examples Directory

This directory contains example structures for testing and demonstrating `lstr` functionality.

## Available Examples

### `sample-directory/` 
A comprehensive test structure demonstrating all `lstr` features:

- **Tree Structure**: Complex nested directories with proper `├──`, `└──`, and `│` connectors
- **File Types**: Various extensions for testing icons (`.rs`, `.js`, `.md`, `.json`, `.yaml`, `.png`, etc.)
- **Hidden Files**: `.env`, `.hidden/` directory, `.secrets` files (use `-a` to show)
- **Gitignore Testing**: `build/`, `node_modules/`, `temp/` directories that should be ignored with `-g`
- **Mixed Content**: Combination of source code, documentation, assets, configuration files

## Usage Examples

```bash
# Basic tree view
./target/debug/lstr examples/sample-directory

# With icons and showing all files (including hidden)
./target/debug/lstr --icons -a examples/sample-directory

# Respect gitignore (hides build/, node_modules/, temp/ directories)  
./target/debug/lstr -g examples/sample-directory

# Directories first with natural sorting
./target/debug/lstr --dirs-first --natural-sort examples/sample-directory

# Show permissions and file sizes
./target/debug/lstr --permissions --size examples/sample-directory

# Limit depth and show git status
./target/debug/lstr --level 2 --git-status examples/sample-directory

# Interactive TUI mode
./target/debug/lstr interactive examples/sample-directory
```

## Expected Output Features

The examples directory should demonstrate:

✅ **Proper Tree Connectors**: `├──` for intermediate entries, `└──` for last entries, `│` for vertical lines  
✅ **Icon Display**: Different icons for file types (requires Nerd Font)  
✅ **Gitignore Respect**: Build artifacts and dependencies properly hidden with `-g`  
✅ **Hidden Files**: Dotfiles and dot-directories shown only with `-a`  
✅ **Sorting Options**: All sorting modes work correctly  
✅ **File Information**: Sizes, permissions, modification times display properly

## Adding New Examples

To add new test cases:

1. Create a new subdirectory under `examples/`
2. Structure it to test specific `lstr` features
3. Document the expected behavior
4. Avoid adding `Cargo.toml` files that would interfere with the build process