# Publishing — Channel 1: PyPI via Maturin

> Completed: 2026-02-13
> Ref: `docs/executing/publishing.md`, Channel 1 (PyPI)
> Tests: 226 passing, zero clippy warnings

---

## Summary

Photon can now be distributed as a Python wheel via PyPI using maturin's `bindings = "bin"` mode. This is the same pattern used by `ruff` and `uv` — the Rust binary is packaged directly inside a platform-specific wheel, installed to the user's `bin/` directory with `pip install photon-ai`.

---

## Files Created

### `pyproject.toml` (workspace root)

Maturin build configuration:
- `bindings = "bin"` — ships the compiled binary, no Python extension module
- `manifest-path = "crates/photon/Cargo.toml"` — points to the CLI crate
- `strip = true` — reduces binary size
- Package name: `photon-ai`
- Metadata: description, license (MIT OR Apache-2.0), keywords, classifiers, repository URL

### `.github/workflows/pypi.yml`

CI workflow triggered on `v*` tags (same as `release.yml`) and `workflow_dispatch`.

**Build matrix** (4 targets):
| Runner | Target |
|--------|--------|
| `macos-14` | `aarch64-apple-darwin` |
| `macos-13` | `x86_64-apple-darwin` |
| `ubuntu-latest` | `x86_64-unknown-linux-gnu` |
| `ubuntu-latest` | `aarch64-unknown-linux-gnu` |

**Jobs:**
1. `build` — uses `PyO3/maturin-action@v1` per target, uploads `.whl` artifacts
2. `publish` — downloads all wheels, publishes to PyPI via trusted publishing (OIDC `id-token: write`, no API token needed)

---

## Files Modified

### `Cargo.toml` (workspace root)

Switched `reqwest` from `native-tls` (default, links OpenSSL dynamically) to `rustls-tls` (statically compiled):

```toml
# Before
reqwest = { version = "0.12", features = ["json", "stream"] }

# After
reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls"] }
```

**Why:** With native-tls, maturin bundled `libssl.so` and `libcrypto.so` into a `.libs/` directory in the wheel. The binary's RPATH (`$ORIGIN/../photon-ai.libs`) doesn't resolve correctly when pip installs the binary to `bin/` and the libs to `site-packages/photon-ai.libs/` — a known limitation of maturin bin bindings with dynamic libraries.

Switching to rustls eliminates the OpenSSL dependency entirely, producing a fully self-contained binary (39MB stripped). This is the same approach used by ruff, uv, and other Rust-based CLI tools distributed via PyPI.

---

## Local Verification

```
$ maturin build --release
📦 Including license file `LICENSE-APACHE`
📦 Including license file `LICENSE-MIT`
🔗 Found bin bindings
📦 Built wheel to target/wheels/photon_ai-0.1.0-py3-none-manylinux_2_39_aarch64.whl

$ unzip -l target/wheels/*.whl
  39362792  photon_ai-0.1.0.data/scripts/photon
     12035  photon_ai-0.1.0.dist-info/METADATA
       107  photon_ai-0.1.0.dist-info/WHEEL
     11357  photon_ai-0.1.0.dist-info/licenses/LICENSE-APACHE
      1070  photon_ai-0.1.0.dist-info/licenses/LICENSE-MIT
       520  photon_ai-0.1.0.dist-info/RECORD

$ pip install target/wheels/*.whl && photon --version
photon 0.1.0
```

---

## Before First Publish

1. **Configure trusted publishing on PyPI**: Go to pypi.org → project settings → "Publishing" → add GitHub Actions as a trusted publisher (repo: `hejijunhao/photon`, workflow: `pypi.yml`, environment: `pypi`)
2. **Create the `pypi` environment** in GitHub repo settings → Environments
3. **Tag a release**: `git tag v0.1.0 && git push --tags` — this triggers both `release.yml` (GitHub Release) and `pypi.yml` (PyPI upload)
4. **Verify**: `pip install photon-ai && photon --version`
