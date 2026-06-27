# difftree task runner
# Run `just` or `just --list` to see available recipes.

# Symlink-install location (already on PATH); the real install uses ~/.cargo/bin via cargo.
bindir  := home_directory() / ".local" / "bin"
binname := "difftree"

# Show available recipes
default:
    @just --list

# Build (debug)
build:
    cargo build

# Build (release)
build-release:
    cargo build --release

# Format the code
format:
    cargo fmt

# Check formatting without modifying files (CI)
format-check:
    cargo fmt --check

# Lint with clippy, treating warnings as errors
lint:
    cargo clippy -- -D warnings

# Typecheck (Rust's compiler front-end; faster than a full build)
typecheck:
    cargo check

# Run the test suite
test:
    cargo test

# Run the binary, forwarding any arguments: `just run -aG .`
run *ARGS:
    cargo run -- {{ARGS}}

# Format, lint, typecheck, and test
all: format lint typecheck test

# Install the release binary to ~/.cargo/bin via cargo
install:
    cargo install --path .

# Uninstall the cargo-installed binary
uninstall:
    cargo uninstall {{binname}}

# Symlink-install: link ~/.local/bin -> the built release binary (auto-updates on rebuild)
install-symlink: build-release
    mkdir -p {{bindir}}
    ln -sf {{justfile_directory()}}/target/release/{{binname}} {{bindir}}/{{binname}}

# Remove the ~/.local/bin symlink (only if it is a symlink)
uninstall-symlink:
    if [ -L {{bindir}}/{{binname}} ]; then rm -f {{bindir}}/{{binname}}; fi
