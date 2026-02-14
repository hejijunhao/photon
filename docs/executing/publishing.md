# Publishing Proposal

> Distribute Photon as a native binary via **pip**, **npm**, and **crates.io** — three channels that together cover Python/ML developers, Node/web developers, and Rust developers.

---

## Strategy Overview

Photon is a compiled Rust binary with an ONNX Runtime dependency. Unlike pure-Python or pure-JS tools, we need to ship **platform-specific native binaries**. The good news: this is a solved problem — tools like `ruff`, `uv`, `esbuild`, and `biome` have paved the way.

| Channel | Target audience | Install command | Priority |
|---------|----------------|-----------------|----------|
| **pip / PyPI** | ML/AI developers, data teams | `pip install photon-imager` | **P0** — lowest effort, best audience fit |
| **npm** | Node/fullstack developers | `npm install -g @photon-ai/photon` | **P1** — broadest reach |
| **cargo install** | Rust developers | `cargo install photon` | **P2** — already works, publish to crates.io |

### Why this order?

1. **pip** is the lowest-effort option thanks to [maturin](https://www.maturin.rs/), which handles cross-platform wheel building automatically. Photon's audience (image processing, AI tagging) overlaps heavily with the Python ecosystem.
2. **npm** has the broadest reach and the esbuild/biome pattern is well-proven, but requires maintaining ~6+ platform-specific packages.
3. **crates.io** is near-zero effort since the workspace is already structured correctly.

---

## Channel 1: PyPI (via maturin)

### How it works

[maturin](https://www.maturin.rs/) builds platform-specific Python wheels containing the Photon binary. When a user runs `pip install photon-imager`, pip selects the wheel matching their OS/arch and places the `photon` binary directly in their Python `bin/` directory — no Python wrapper, no runtime overhead.

This is exactly how **ruff** and **uv** (from Astral) distribute their Rust binaries to millions of Python users.

### What we need to add

**`pyproject.toml`** (workspace root):

```toml
[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[project]
name = "photon-imager"
version = "0.1.0"
description = "Fast image processing CLI with AI-powered tagging and embeddings"
readme = "README.md"
license = { text = "MIT OR Apache-2.0" }
requires-python = ">=3.8"
keywords = ["image", "tagging", "embedding", "siglip", "ai", "cli"]
classifiers = [
    "Development Status :: 4 - Beta",
    "Environment :: Console",
    "Intended Audience :: Developers",
    "License :: OSI Approved :: MIT License",
    "License :: OSI Approved :: Apache Software License",
    "Programming Language :: Rust",
    "Topic :: Multimedia :: Graphics",
    "Topic :: Scientific/Engineering :: Artificial Intelligence",
]

[project.urls]
Repository = "https://github.com/hejijunhao/photon"
Issues = "https://github.com/hejijunhao/photon/issues"

[tool.maturin]
bindings = "bin"
manifest-path = "crates/photon/Cargo.toml"
strip = true
```

The key setting is `bindings = "bin"` — this tells maturin we're shipping a standalone binary, not a Python extension module.

### CI workflow

**`.github/workflows/pypi.yml`**:

```yaml
name: PyPI

on:
  push:
    tags: ['v*']
  workflow_dispatch:

permissions:
  contents: read

jobs:
  build:
    name: Build wheel (${{ matrix.target }})
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - os: macos-14
            target: aarch64-apple-darwin
          - os: macos-13
            target: x86_64-apple-darwin
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
    steps:
      - uses: actions/checkout@v4

      - uses: PyO3/maturin-action@v1
        with:
          target: ${{ matrix.target }}
          manylinux: auto
          args: --release --out dist

      - uses: actions/upload-artifact@v4
        with:
          name: wheel-${{ matrix.target }}
          path: dist/*.whl

  publish:
    name: Publish to PyPI
    needs: build
    runs-on: ubuntu-latest
    environment: pypi
    permissions:
      id-token: write  # trusted publishing
    steps:
      - uses: actions/download-artifact@v4
        with:
          path: dist
          merge-multiple: true

      - uses: PyO3/maturin-action@v1
        with:
          command: upload
          args: --non-interactive --skip-existing dist/*
```

### Result

```bash
pip install photon-imager        # install
photon process image.jpg         # use — binary is on PATH
pip install --upgrade photon-imager  # update
```

---

## Channel 2: npm (esbuild pattern)

### How it works

Following the pattern established by **esbuild**, **biome**, and **turbo**: one lightweight wrapper package + platform-specific binary packages via `optionalDependencies`. npm automatically installs only the package matching the user's OS/arch.

### Package structure

```
npm/
├── photon/                          # Main wrapper package
│   ├── package.json
│   └── bin/
│       └── photon                   # Small JS shim that finds and executes the binary
├── photon-darwin-arm64/             # macOS Apple Silicon
│   ├── package.json
│   └── bin/
│       └── photon                   # Actual native binary
├── photon-darwin-x64/               # macOS Intel
│   ├── package.json
│   └── bin/
│       └── photon
├── photon-linux-x64/                # Linux x86_64
│   ├── package.json
│   └── bin/
│       └── photon
└── photon-linux-arm64/              # Linux ARM
    ├── package.json
    └── bin/
        └── photon
```

**Main package (`npm/photon/package.json`)**:

```json
{
  "name": "@photon-ai/photon",
  "version": "0.1.0",
  "description": "Fast image processing CLI with AI-powered tagging and embeddings",
  "license": "MIT OR Apache-2.0",
  "repository": "https://github.com/hejijunhao/photon",
  "bin": {
    "photon": "bin/photon"
  },
  "optionalDependencies": {
    "@photon-ai/photon-darwin-arm64": "0.1.0",
    "@photon-ai/photon-darwin-x64": "0.1.0",
    "@photon-ai/photon-linux-x64": "0.1.0",
    "@photon-ai/photon-linux-arm64": "0.1.0"
  }
}
```

**Platform package (`npm/photon-darwin-arm64/package.json`)**:

```json
{
  "name": "@photon-ai/photon-darwin-arm64",
  "version": "0.1.0",
  "description": "Photon binary for macOS ARM64",
  "license": "MIT OR Apache-2.0",
  "os": ["darwin"],
  "cpu": ["arm64"],
  "bin": {
    "photon": "bin/photon"
  }
}
```

The `os` and `cpu` fields are what npm uses to filter — only the matching platform package gets installed.

**Wrapper script (`npm/photon/bin/photon`)**:

```js
#!/usr/bin/env node
const { execFileSync } = require("child_process");
const { join } = require("path");

const PLATFORMS = {
  "darwin-arm64":  "@photon-ai/photon-darwin-arm64",
  "darwin-x64":    "@photon-ai/photon-darwin-x64",
  "linux-x64":     "@photon-ai/photon-linux-x64",
  "linux-arm64":   "@photon-ai/photon-linux-arm64",
};

const key = `${process.platform}-${process.arch}`;
const pkg = PLATFORMS[key];
if (!pkg) {
  console.error(`Unsupported platform: ${key}`);
  process.exit(1);
}

const bin = require.resolve(`${pkg}/bin/photon`);
execFileSync(bin, process.argv.slice(2), { stdio: "inherit" });
```

### CI workflow

Add a job to the release workflow that:
1. Copies the built binary into each platform package directory
2. Runs `npm publish` for each package (platform packages first, then main package)

### Result

```bash
npm install -g @photon-ai/photon  # install globally
npx @photon-ai/photon process .   # or run via npx
```

---

## Channel 3: crates.io

### What we need

The workspace is already structured correctly. Two things to do:

1. **Publish `photon-core` first** (it's a dependency of `photon`)
2. **Publish `photon`** with `photon-core` as a crates.io dependency instead of `path`

Update `crates/photon/Cargo.toml`:

```toml
[dependencies]
photon-core = { version = "0.1.0", path = "../photon-core" }
```

The `version` field is used when publishing to crates.io; the `path` is used for local development. Both can coexist.

### Result

```bash
cargo install photon
```

---

## Implementation Plan

### Phase A: PyPI (1-2 sessions)

1. Add `pyproject.toml` to workspace root
2. Test locally with `maturin build --release` and `maturin develop`
3. Create PyPI account + API token (or set up trusted publishing)
4. Add `.github/workflows/pypi.yml`
5. Tag a release, verify wheel appears on PyPI
6. Test: `pip install photon-imager && photon --version`

### Phase B: npm (2-3 sessions)

1. Create `npm/` directory structure with wrapper + platform packages
2. Add a `scripts/publish-npm.sh` that copies release binaries into the right package dirs
3. Create npm org `@photon-ai`
4. Add npm publish step to release workflow
5. Test: `npx @photon-ai/photon --version`

### Phase C: crates.io (1 session)

1. Add `version` field to path dependency in `crates/photon/Cargo.toml`
2. Ensure `photon-core` has all required metadata (`description`, `license`, `repository`)
3. `cargo publish -p photon-core` then `cargo publish -p photon`
4. Test: `cargo install photon`

---

## Name Availability

Before proceeding, verify name availability on each registry:

| Registry | Desired name | Fallback |
|----------|-------------|----------|
| PyPI | `photon-imager` | `photon-image`, `photon-tag` |
| npm | `@photon-ai/photon` | Scoped names are always available if we own the org |
| crates.io | `photon` | `photon-image`, `photon-ai` |

> **Note:** The name `photon` is likely taken on PyPI and crates.io. `photon-ai` collides with `photonai` (PyPI name normalization). Using `photon-imager` avoids conflicts and clarifies the tool's purpose.

---

## Summary

| What | How | Effort | Impact |
|------|-----|--------|--------|
| PyPI | maturin + trusted publishing | Low | High — ML/AI audience |
| npm | esbuild-style optionalDependencies | Medium | High — broadest reach |
| crates.io | `cargo publish` | Low | Low — Rust developers only |

**Recommended order: PyPI → npm → crates.io**

PyPI first because maturin makes it almost trivial and the audience fit is perfect. npm second because it has the broadest reach but requires more scaffolding (multiple packages). crates.io last because `cargo install` from git already works.
