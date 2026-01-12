#!/bin/sh
set -euo pipefail

coverage_root="target/coverage"
lcov_path="${coverage_root}/lcov.info"
html_dir="${coverage_root}/html"

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  cargo install cargo-llvm-cov --locked
fi

if command -v rustup >/dev/null 2>&1; then
  rustup component add llvm-tools-preview
fi

mkdir -p "${coverage_root}"

cargo llvm-cov --workspace --all-features --lcov --output-path "${lcov_path}"
cargo llvm-cov --workspace --all-features --html --output-dir "${html_dir}"

echo "coverage: ${lcov_path}"
echo "coverage: ${html_dir}"
