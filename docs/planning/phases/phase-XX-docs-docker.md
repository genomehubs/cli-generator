# Phase XX: Docs Docker Image and Multi-Site Strategy

**Status:** Design
**Depends on:** Phase 17 (recipe docs), Phase 6g (Quarto docs generation)
**Blocks:** Nothing downstream
**Estimated scope:** 1 `Dockerfile.docs`, 1 new CI job, 1 `scripts/build_docs_image.sh`

---

## 1. Problem statement

The API image (`Dockerfile.api`) is site-agnostic: a single binary is
parameterised at run-time via a mounted TOML. Docs cannot work this way
because:

- Quarto compiles `.qmd` → static HTML at **build time**
- The HTML embeds site-specific content (API base URL, index names, SDK name,
  notice text, recipe code snippets, etc.)
- There is no runtime substitution layer

Each site therefore needs its own docs image. The challenge is to build those
images without duplicating the heavy Rust/Quarto compile work.

---

## 2. Architecture overview

```
cli-generator repo
│
├── cargo run -- new goat           # 1. Generate goat-cli source
│     └── workdir/goat-cli/
│           ├── src/
│           ├── docs/               # 2. Quarto source (.qmd files)
│           └── js/, r/, python/
│
├── quarto render docs/             # 3. Render to static HTML
│     └── workdir/goat-cli/docs/_site/
│
└── Dockerfile.docs                 # 4. Copy _site/ into nginx image
      └── ghcr.io/genomehubs/goat-docs:latest
```

The key insight: steps 1–3 happen in CI (inside an intermediate Docker build
stage or as separate CI steps), step 4 is just `nginx:alpine` + `COPY`.

---

## 3. Proposed Dockerfile.docs

```dockerfile
# ── Stage 1: Generate site source ───────────────────────────────────────────
FROM rust:1.81-bookworm AS generator
WORKDIR /build

# Install Quarto (required for stage 2)
ARG QUARTO_VERSION=1.6.42
RUN curl -L "https://github.com/quarto-dev/quarto-cli/releases/download/v${QUARTO_VERSION}/quarto-${QUARTO_VERSION}-linux-amd64.deb" -o quarto.deb \
    && dpkg -i quarto.deb && rm quarto.deb

# Copy cli-generator source
COPY . .

# Build the cli-generator binary
RUN cargo build --release --bin cli-generator

# ── Stage 2: Generate and render site docs ───────────────────────────────────
FROM generator AS renderer
ARG SITE=goat

# Generate the site's source tree
RUN ./target/release/cli-generator new "$SITE" \
        --output-dir /workdir \
        --config sites/ \
        --no-wasm                      # skip WASM — not needed for docs

# Render the Quarto docs to static HTML
RUN cd "/workdir/${SITE}-cli/docs" && quarto render --no-cache

# ── Stage 3: Serve with nginx ────────────────────────────────────────────────
FROM nginx:1.27-alpine AS docs
ARG SITE=goat
ARG DOCS_BASE_PATH=/docs

# Copy rendered static site
COPY --from=renderer /workdir/${SITE}-cli/docs/_site /usr/share/nginx/html${DOCS_BASE_PATH}

# nginx config: serve from /docs, let unknown paths fall back to 404.html
COPY docker/nginx-docs.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
```

A `docker/nginx-docs.conf` template handles the `DOCS_BASE_PATH`:

```nginx
server {
    listen 80;
    root /usr/share/nginx/html;
    index index.html;

    location /docs/ {
        try_files $uri $uri/ $uri.html =404;
    }

    error_page 404 /docs/404.html;
}
```

**Build command:**

```bash
docker build \
  --build-arg SITE=goat \
  -f Dockerfile.docs \
  -t ghcr.io/genomehubs/goat-docs:latest .
```

---

## 4. CI integration

### 4a. New job: `docs-image`

Add to `.github/workflows/ci.yml` (or a separate `docs.yml`):

```yaml
docs-image:
  name: Build and push docs image (${{ matrix.site }})
  # Only build on main / tags to avoid burning CI minutes on every PR
  if: github.ref == 'refs/heads/main' || startsWith(github.ref, 'refs/tags/')
  runs-on: ubuntu-latest
  permissions:
    contents: read
    packages: write
  strategy:
    matrix:
      site: [goat] # extend to [goat, boat, lepbase] as each site matures
  steps:
    - uses: actions/checkout@v4

    - name: Log in to GHCR
      uses: docker/login-action@v3
      with:
        registry: ghcr.io
        username: ${{ github.actor }}
        password: ${{ secrets.GITHUB_TOKEN }}

    - name: Build and push docs image
      uses: docker/build-push-action@v5
      with:
        context: .
        file: Dockerfile.docs
        build-args: |
          SITE=${{ matrix.site }}
        push: true
        tags: |
          ghcr.io/genomehubs/${{ matrix.site }}-docs:latest
          ghcr.io/genomehubs/${{ matrix.site }}-docs:${{ github.sha }}
        cache-from: type=gha
        cache-to: type=gha,mode=max
```

### 4b. PR check: generate-only (no render)

For speed on PRs, a lighter check can verify docs _generate_ (Rust renders the
`.qmd` source) without running `quarto render` (which requires Quarto + TinyTeX):

```yaml
- name: Verify docs generation (no Quarto render)
  run: |
    cargo run -- new goat --output-dir /tmp/goat-cli --config sites/ --no-wasm
    test -f /tmp/goat-cli/goat-cli/docs/index.qmd
    test -f /tmp/goat-cli/goat-cli/docs/reference/query-builder.qmd
```

This catches template errors without the Quarto install overhead.

---

## 5. Multi-site strategy

The goal is a `/docs` path under each site's domain (or subdomain):

| Site    | URL                        | Image                             |
| ------- | -------------------------- | --------------------------------- |
| GoaT    | `goat.genomehubs.org/docs` | `ghcr.io/genomehubs/goat-docs`    |
| BoaT    | `boat.genomehubs.org/docs` | `ghcr.io/genomehubs/boat-docs`    |
| LepBase | `lepbase.org/docs`         | `ghcr.io/genomehubs/lepbase-docs` |

### 5a. Extending to new sites

Adding a site to the matrix is the only CI change needed. The
`Dockerfile.docs` is site-agnostic: `--build-arg SITE=boat` generates and
renders a completely different docs tree.

### 5b. Deployment

Two options:

**Option 1 — Separate docs container per site (recommended)**
Each site deploys its own `<site>-docs` container alongside the API container.
A reverse proxy (nginx / Caddy / Traefik) routes `/docs/` traffic to the docs
container and `/api/` to the API container. Clean separation; images are
independently deployable.

**Option 2 — Embed docs in API image**
Copy `_site/` into the API image and serve from the same binary/nginx.
Simpler operationally but couples docs and API release cycles. Not recommended
for sites where docs update more frequently than the API.

### 5c. Incremental rollout plan

1. **GoaT only** — implement `Dockerfile.docs`, CI job with `matrix: [goat]`
2. **Smoke-test** — confirm the image builds and serves `GET /docs/` → 200
3. **Deployment** — add docs container to GoaT compose/k8s manifest alongside API
4. **Extend** — add `boat` and `lepbase` to the CI matrix once their configs are stable
5. **Docs-only update workflow** — add a separate `docs-deploy.yml` workflow
   triggered manually or on `sites/*.yaml` changes so docs can be pushed without
   a full code release

---

## 6. Open questions

- **`--no-wasm` flag** — `cli-generator new` does not yet have a `--no-wasm`
  flag; docs generation does not require the WASM build, so adding this flag
  would speed up the docs build by ~5 min. This is a pre-requisite for
  efficient CI.
- **TinyTeX** — `quarto render` with PDF output requires TinyTeX (large install).
  The HTML-only docs target does not; confirm `_quarto.yml.tera` does not
  request PDF output before committing to this approach.
- **Quarto version pinning** — pin to a specific Quarto version in the
  Dockerfile and in a dev note (`.quarto-version` file or `AGENTS.md` note) to
  avoid render drift between environments.
- **Cache busting** — because the docs image embeds `cargo run -- new` output,
  any change to a site's YAML, a template, or the generator binary will produce
  a different image. GitHub Actions layer caching handles this automatically
  when the `cache-from/cache-to: type=gha` strategy is used.

---

## 7. Acceptance criteria

- [ ] `docker build --build-arg SITE=goat -f Dockerfile.docs .` completes on a
      clean checkout (no pre-built artifacts required)
- [ ] `docker run --rm -p 8080:80 ghcr.io/genomehubs/goat-docs` serves
      `GET http://localhost:8080/docs/` → 200 with valid HTML
- [ ] CI `docs-image` job pushes to GHCR on merge to `main`
- [ ] PR check verifies docs `.qmd` generation without Quarto (fast)
- [ ] `docker/nginx-docs.conf` is committed to the repository
- [ ] `Dockerfile.docs` passes `hadolint` with no `DL` errors

---

## 8. Implementation checklist

- [ ] Add `--no-wasm` flag to `cargo run -- new` (speeds up docs-only builds)
- [ ] Add `docker/nginx-docs.conf`
- [ ] Add `Dockerfile.docs` with three stages (generator / renderer / nginx)
- [ ] Add `docs-image` job to `.github/workflows/ci.yml`
- [ ] Add PR-level docs generation check to `generated-cli-tests` or new job
- [ ] Smoke-test locally: build image, run container, verify `GET /docs/`
- [ ] Update `RELEASING.md` to document docs image versioning
