# Sample Directory for Testing lstr

This directory provides a comprehensive test structure for demonstrating and testing `lstr` functionality.

## Structure Overview

```
sample-directory/
├── docs/                    # Documentation files
│   ├── README.md
│   ├── api.md
│   └── changelog.txt
├── src/                     # Source code files  
│   ├── main.rs
│   ├── lib.rs
│   ├── utils/
│   │   ├── helpers.rs
│   │   └── config.json
│   └── tests/
│       ├── integration.rs
│       └── unit_tests.rs
├── assets/                  # Various file types for icon testing
│   ├── images/
│   │   ├── logo.png
│   │   ├── banner.jpg
│   │   └── favicon.ico
│   ├── data/
│   │   ├── sample.csv
│   │   ├── config.yaml
│   │   └── database.sqlite
│   └── fonts/
│       ├── regular.ttf
│       └── bold.woff2
├── .hidden/                 # Hidden directory (test -a flag)
│   ├── .secrets
│   └── .config
├── build/                   # Build artifacts (gitignored)
│   ├── debug/
│   │   └── app.exe
│   └── release/
│       └── app
├── node_modules/            # Dependencies (gitignored)
│   └── package/
│       └── index.js
├── temp/                    # Temporary files (gitignored)
│   ├── cache.tmp
│   └── log.txt
├── .gitignore              # Test gitignore functionality
├── .env                    # Environment file (hidden)
├── Cargo.toml             # Rust manifest
├── package.json           # Node manifest  
├── LICENSE                # License file
└── CHANGELOG.md          # Changelog
```

## Testing Features

### Tree Structure (`lstr examples/sample-directory`)
- Tests proper `├──`, `└──`, and `│` connectors
- Various nesting depths
- Mixed files and directories

### Icons (`lstr -i examples/sample-directory`)  
- Programming languages: `.rs`, `.js`, `.py`
- Images: `.png`, `.jpg`, `.ico`
- Data: `.json`, `.yaml`, `.csv`, `.sqlite`
- Documents: `.md`, `.txt`
- Archives and executables

### Gitignore (`lstr -g examples/sample-directory`)
- `build/` directory ignored
- `node_modules/` ignored  
- `temp/` files ignored
- `*.log` patterns ignored

### Hidden Files (`lstr -a examples/sample-directory`)
- `.hidden/` directory
- `.env`, `.secrets` files
- Compare with/without `-a` flag

### Sorting Options
- `--dirs-first`: Directories before files
- `--natural-sort`: Proper numeric ordering
- `--sort name|size|modified|extension`: Different sort criteria

### Other Features
- `--permissions`: Unix file permissions
- `--size`: File sizes
- `--depth`: Limit traversal depth
- Git status integration (if initialized as git repo)

## Usage Examples

```bash
# Basic tree view
lstr examples/sample-directory

# With icons and all files  
lstr -ia examples/sample-directory

# Respect gitignore
lstr -g examples/sample-directory  

# Directories first with natural sorting
lstr --dirs-first --natural-sort examples/sample-directory

# Show permissions and sizes
lstr -ps examples/sample-directory

# Limit depth  
lstr -L 2 examples/sample-directory
```