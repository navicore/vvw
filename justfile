# VVW Build System
#
# This is the SOURCE OF TRUTH for all build/test/lint operations.
# GitHub Actions calls these recipes directly - no duplication!

# Default recipe: show available commands
default:
    @just --list

# Build everything
build:
    @echo "Building workspace..."
    cargo build --workspace
    @echo "Build complete."

# Build release
build-release:
    @echo "Building workspace (release)..."
    cargo build --workspace --release
    @echo "Release build complete."

# Run all Rust unit tests
test:
    @echo "Running tests..."
    cargo test --workspace --all-targets

# Run clippy on all workspace members
lint:
    @echo "Running clippy..."
    cargo clippy --workspace --all-targets -- -D warnings

# Format all code
fmt:
    @echo "Formatting code..."
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    @echo "Checking code formatting..."
    cargo fmt --all -- --check

# Run all CI checks (same as GitHub Actions!)
# This is what developers should run before pushing
ci: fmt-check lint test build
    @echo ""
    @echo "All CI checks passed!"
    @echo "  - Code formatting"
    @echo "  - Clippy lints"
    @echo "  - Unit tests"
    @echo "  - Build"
    @echo ""
    @echo "Safe to push to GitHub - CI will pass."

# Development: quick format + build + test
dev: fmt build test

# Run the game
run:
    cargo run

# Run with debug logging
run-debug:
    cargo run -- --debug

# Generate documentation
doc:
    cargo doc --workspace --no-deps --open

# Show test output (verbose)
test-verbose:
    cargo test --workspace -- --nocapture

# Clean all build artifacts
clean:
    @echo "Cleaning build artifacts..."
    cargo clean
    @echo "Clean complete."

# Build WASM player (requires trunk + wasm32-unknown-unknown target)
build-wasm:
    @echo "Building WASM player..."
    cd crates/vvw-web && trunk build --release
    @echo "WASM build complete."

# Run WASM tests for vvw-web (requires wasm-pack)
test-wasm:
    @echo "Running WASM tests..."
    wasm-pack test --node crates/vvw-web --lib

# Assemble deploy directory from trunk dist output (no album data)
assemble-deploy:
    @echo "Assembling deploy directory..."
    mkdir -p deploy
    cp crates/vvw-web/dist/index.html deploy/index.html
    cp crates/vvw-web/dist/*.js deploy/ 2>/dev/null || true
    cp crates/vvw-web/dist/*.wasm deploy/ 2>/dev/null || true
    printf '/*  /index.html  200\n' > deploy/_redirects
    @echo "Deploy directory assembled."

# Check for outdated dependencies
outdated:
    cargo outdated --workspace
