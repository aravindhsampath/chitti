.PHONY: all fmt lint test check audit clean

# Default target
all: fmt lint check test

# Check formatting
fmt:
	cargo fmt --all -- --check

# Run linter (Clippy)
lint:
	cargo clippy --all-targets --all-features -- -D warnings

# Basic compilation check
check:
	cargo check --all-targets --all-features

# Run all tests (unit + integration)
test:
	cargo test --all-targets --all-features

# Security audit (requires cargo-audit to be installed)
# If not installed, this will fail gracefully with a message
audit:
	@if command -v cargo-audit >/dev/null 2>&1; then \
		cargo audit; \
	else \
		echo "cargo-audit not found. Skipping security audit. Install with: cargo install cargo-audit"; \
	fi

# Clean build artifacts
clean:
	cargo clean

# Run everything including audit
full-check: audit all
