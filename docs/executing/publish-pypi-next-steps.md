# PyPI Publishing — Next Steps

> Prerequisites: `pyproject.toml`, `.github/workflows/pypi.yml`, and rustls switch are already in place.

---

## 1. Create `pypi` environment in GitHub

Go to **GitHub repo → Settings → Environments → New environment** → name it `pypi`.

This is referenced by `environment: pypi` in the workflow. It gates the publish job.

## 2. Configure trusted publishing on PyPI

Go to **pypi.org → your account → Publishing** (or create the `photon-ai` project first via "Create a new pending publisher"):

| Field | Value |
|-------|-------|
| PyPI project name | `photon-ai` |
| Owner | `hejijunhao` |
| Repository | `photon` |
| Workflow name | `pypi.yml` |
| Environment name | `pypi` |

This allows the GitHub Actions workflow to publish without an API token (OIDC).

## 3. Tag and push a release

```bash
git tag v0.1.0
git push --tags
```

This triggers both:
- `release.yml` → GitHub Release with native binaries
- `pypi.yml` → builds wheels for 4 platforms, publishes to PyPI

## 4. Verify

```bash
pip install photon-ai
photon --version
```
