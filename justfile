# difftree task runner
# Run `just` or `just --list` to see available recipes.

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
