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

# --- Album Creation ---

# Create a new album from a directory of audio files (opens $EDITOR for metadata)
create-album *ARGS:
    cargo run -p vvw-deploy --release -- create {{ARGS}}

# Export maze layout as a PNG mask for artwork creation
export-maze ALBUM *ARGS:
    cargo run -p vvw-deploy --release -- export-maze {{ALBUM}} {{ARGS}}

# --- WASM / Web ---

# Find wasm-opt: prefer PATH, fall back to trunk's cached copy
WASM_OPT := `which wasm-opt 2>/dev/null || find ~/Library/Caches/dev.trunkrs.trunk ~/.cache/trunk -name wasm-opt -type f 2>/dev/null | head -1 || echo wasm-opt`

# Build WASM web player (release, size-optimized for Cloudflare Pages 25 MiB limit)
build-web:
    @echo "Cleaning dist for fresh build..."
    rm -rf crates/vvw-web/dist
    @echo "Building WASM player..."
    cd crates/vvw-web && trunk build --release --cargo-profile release-wasm
    @echo "Running wasm-opt ({{WASM_OPT}})..."
    {{WASM_OPT}} --enable-bulk-memory --enable-mutable-globals --enable-sign-ext \
        --enable-nontrapping-float-to-int --enable-simd --enable-reference-types \
        --enable-multivalue -Oz \
        -o crates/vvw-web/dist/vvw-web-opt.wasm \
        crates/vvw-web/dist/*_bg.wasm
    @# Replace the original with the optimized version
    mv crates/vvw-web/dist/vvw-web-opt.wasm $(ls crates/vvw-web/dist/*_bg.wasm)
    @echo "WASM build complete."

# Build WASM web player (dev, faster iteration)
build-web-dev:
    @echo "Building WASM player (dev)..."
    cd crates/vvw-web && trunk build
    @echo "WASM dev build complete."

# Check web player compiles for WASM target
check-web:
    cargo check -p vvw-web --target wasm32-unknown-unknown

# Run WASM tests for vvw-web (requires wasm-pack)
test-wasm:
    @echo "Running WASM tests..."
    wasm-pack test --node crates/vvw-web --lib

# --- Deploy (Cloudflare Pages + R2) ---

# R2 public URL for audio streaming
R2_URL := "https://pub-5345c95a0bcc43f1a8702037c4d051d6.r2.dev"

# Site base URL for og:url meta tags
SITE_URL := "https://vvw-2c3.pages.dev"

# List saved projects
list-projects:
    cargo run -p vvw-deploy --release -- list

# Assemble web player + album for local preview (includes audio files)
assemble-local ALBUM OUTPUT="deploy":
    just build-web
    cargo run -p vvw-deploy --release -- assemble {{ALBUM}} --output {{OUTPUT}}

# Assemble web player + specific album for Cloudflare (audio served from R2)
assemble ALBUM OUTPUT="deploy":
    just build-web
    cargo run -p vvw-deploy --release -- assemble {{ALBUM}} --output {{OUTPUT}} --audio-base-url {{R2_URL}} --site-url {{SITE_URL}}

# Assemble web player + ALL saved albums for Cloudflare
assemble-all OUTPUT="deploy":
    just build-web
    cargo run -p vvw-deploy --release -- assemble --all --output {{OUTPUT}} --audio-base-url {{R2_URL}} --site-url {{SITE_URL}}

# Upload audio files to R2
upload-audio ALBUM:
    cargo run -p vvw-deploy --release -- upload-audio {{ALBUM}}

# Local preview server (run assemble-local first)
preview OUTPUT="deploy":
    cargo run -p vvw-deploy --release -- preview --output {{OUTPUT}}

# Deploy to Cloudflare Pages
deploy-pages PROJECT="vvw" OUTPUT="deploy":
    cargo run -p vvw-deploy --release -- deploy --output {{OUTPUT}} --project {{PROJECT}}

# Deploy all albums: assemble all saved projects and deploy to Pages
deploy:
    just assemble-all
    just deploy-pages

# Clean an album from the deploy directory
clean-album ALBUM OUTPUT="deploy":
    cargo run -p vvw-deploy --release -- clean {{ALBUM}} --output {{OUTPUT}}

# Delete an album everywhere: local deploy dir, R2 audio, then redeploy Pages
delete-album ALBUM OUTPUT="deploy":
    cargo run -p vvw-deploy --release -- clean {{ALBUM}} --output {{OUTPUT}}
    cargo run -p vvw-deploy --release -- delete-audio {{ALBUM}}
    just deploy-pages

# Delete audio files from R2 only
delete-audio ALBUM:
    cargo run -p vvw-deploy --release -- delete-audio {{ALBUM}}

# --- Misc ---

# Check for outdated dependencies
outdated:
    cargo outdated --workspace
