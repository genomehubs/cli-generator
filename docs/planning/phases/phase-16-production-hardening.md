# Phase 16: Production Hardening — Security, Rate-Limiting, and Redis Caching

**Depends on:** Phase 8 (Swagger customisation complete), Phase 9 (URL query strings)
**Blocks:** Public GoaT v3 launch
**Estimated scope:** ~8 files modified, 4 new source files, 1 new Docker Compose snippet

---

## 1. Goal

Harden the v3 API server for production co-deployment alongside the existing v2
GoaT API. The three pillars are:

| Pillar                 | What it gives us                                                                      |
| ---------------------- | ------------------------------------------------------------------------------------- |
| **Security hardening** | Correct HTTP response headers, request-size limits, binding, no secret leakage        |
| **Rate limiting**      | Per-IP throttling on expensive endpoints with RFC 7807 error bodies                   |
| **Redis caching**      | ES-result caching shared between containers; v3-scoped keys that cannot clash with v2 |

---

## 2. Audit findings

### 2.1 Security gaps (current state)

| Finding                                                                                               | Severity | Location                                |
| ----------------------------------------------------------------------------------------------------- | -------- | --------------------------------------- |
| Binds to `0.0.0.0:3000` in `main()` but listens on `127.0.0.1:3000` in addr variable (dead code)      | Low      | `main.rs:334`                           |
| No `Content-Security-Policy` or `X-Frame-Options` headers on any response                             | Medium   | all routes                              |
| No maximum request body size — a client can send an arbitrarily large POST body                       | High     | `main.rs` (no `RequestBodyLimit` layer) |
| `reqwest::Client` uses default timeouts (infinite) for ES requests                                    | High     | `main.rs`, `es_client.rs`               |
| `ES_INTEGRATION_CONFIG` path accepted without path-traversal guard                                    | Low      | `main.rs:124`                           |
| `swagger-ui` serves at `/swagger-ui` with no option to disable in production                          | Low      | `main.rs:319`                           |
| `./target/openapi.json` written to filesystem unconditionally; fails silently in read-only containers | Low      | `main.rs:333`                           |
| Docker image runs `cargo build` as root inside builder stage                                          | Low      | `Dockerfile.api`                        |
| `EXPOSE 3000` with no port remapping advice                                                           | Info     | `Dockerfile.api`                        |
| No structured logging format (JSON) or log level configuration                                        | Medium   | `main.rs:110`                           |

### 2.2 Rate-limiting gaps

- No rate limiting exists. The batch endpoints (`/count/batch`, `/search/batch`,
  `/report`) each fan out to many ES requests and are the highest risk for abuse.
- No request-queue depth limit, no per-client concurrency limit.

### 2.3 Caching gaps

- `MetadataCache` (in-process `RwLock`) is populated at startup and
  refreshed only by restart. It does not survive process crashes.
- There is **no result-level cache** for search/count queries. Identical
  queries from the UI and SDK re-hit ES every time.
- The v2 GoaT API uses Redis with keys structured as
  `genomehubs:v2:<endpoint>:<hash>`. Any v3 cache must use a **distinct
  prefix** (`genomehubs:v3:`) to guarantee zero collision even when both
  versions target the same Redis instance.

### 2.4 Dockerfile gaps

| Finding                                                                                                                                         | Severity                          |
| ----------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| `rust:1.81-bookworm` builder — toolchain pinned to old version; mismatches workspace `edition = "2021"` edition support note                    | Low                               |
| `COPY src/ ./src/` and `COPY templates/ ./templates/` copy the cli-generator source into the API image unnecessarily — only `crates/` is needed | Medium (image bloat, slow builds) |
| No `.dockerignore` — copies `target/`, `workdir/`, `sites/`, `artifacts/` etc. into build context                                               | High (very slow builds)           |
| `CMD ["/app/genomehubs-api"]` with no ability to pass args/config path via `ENTRYPOINT`                                                         | Low                               |
| Health-check URL uses `localhost:3000` not `127.0.0.1:3000`                                                                                     | Info                              |
| Image labels (`LABEL org.opencontainers.image.*`) absent                                                                                        | Info                              |

---

## 3. Planned work

### Phase 16-A: Security hardening (~2 days)

#### 16-A-1: Security headers middleware

Add a single Axum `Tower` layer that injects standard security headers on
every response.

**New file:** `crates/genomehubs-api/src/middleware/security_headers.rs`

```rust
// Headers to add on every response:
// X-Content-Type-Options: nosniff
// X-Frame-Options: DENY
// X-XSS-Protection: 0                       (deprecated; set to 0 per OWASP)
// Referrer-Policy: strict-origin-when-cross-origin
// Content-Security-Policy: default-src 'self'; ...  (permissive enough for Swagger UI)
// Strict-Transport-Security: max-age=31536000; includeSubDomains  (only if TLS)
// Permissions-Policy: geolocation=(), microphone=(), camera=()
```

The CSP must allow Swagger UI's inline scripts and external fonts. A suitable
starting policy (reviewed against Swagger UI 5.x):

```
default-src 'self';
script-src 'self' 'unsafe-inline';
style-src 'self' 'unsafe-inline';
img-src 'self' data: https://validator.swagger.io;
connect-src 'self'
```

**Wire-up location:** `main.rs` — add `.layer(security_headers::layer())` after
the `Extension(state)` layer.

**Touch-points:**

- `crates/genomehubs-api/src/middleware/security_headers.rs` — new
- `crates/genomehubs-api/src/middleware/mod.rs` — new (`pub mod security_headers;`)
- `crates/genomehubs-api/src/main.rs` — add `mod middleware;` + `.layer(middleware::security_headers::layer())`
- `crates/genomehubs-api/Cargo.toml` — add `tower = { version = "0.4", features = ["util"] }` and `http = "1.0"`

#### 16-A-2: Request body size limit

Axum ships `axum::extract::DefaultBodyLimit`. Set a global limit of **1 MiB**
(generous for batch queries, prevents abuse).

**Touch-points:**

- `main.rs` — add `.layer(DefaultBodyLimit::max(1_024 * 1_024))` to the router
- Import `use axum::extract::DefaultBodyLimit;`

#### 16-A-3: ES client timeouts

Configure `reqwest::Client` with explicit timeouts so a slow ES cluster cannot
hold handler threads indefinitely.

Suggested values (configurable via `es_integration.toml`):

- `connect_timeout = 5s`
- `timeout = 30s` (overall request timeout per ES query)

**Touch-points:**

- `crates/genomehubs-api/src/main.rs` — extend `EsConfig` with
  `es_connect_timeout_secs: Option<u64>` and `es_request_timeout_secs: Option<u64>`;
  build `reqwest::Client` with those values
- `config/es_integration.toml` — document new optional fields

#### 16-A-4: Bind address and port configuration

The dead-code `addr` variable and the hard-coded `0.0.0.0:3000` in
`TcpListener::bind` are confusing. Unify them and make the bind address
configurable.

**Touch-points:**

- `main.rs` — add `EsConfig.listen_addr: Option<String>` (default `"0.0.0.0:3000"`)
- Remove the unused `SocketAddr` import (already shadowed by the string bind)
- `config/es_integration.toml` — document `listen_addr`

#### 16-A-5: Structured (JSON) logging

Replace `tracing_subscriber::fmt::init()` with a subscriber that:

- Emits JSON when `LOG_FORMAT=json` (production default in Docker)
- Emits human-readable text otherwise (dev default)
- Respects `RUST_LOG` env var for level filtering

**Touch-points:**

- `crates/genomehubs-api/Cargo.toml` — add `tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }`
- `main.rs` — replace `tracing_subscriber::fmt::init()` with conditional JSON/text subscriber

#### 16-A-6: `./target/openapi.json` write

In a read-only container filesystem (common in production), this write fails
silently. It is only useful during development. Guard it behind an env var
`WRITE_OPENAPI_JSON=1` (off by default).

**Touch-points:**

- `main.rs` — wrap the `std::fs::write` block in
  `if std::env::var("WRITE_OPENAPI_JSON").is_ok() { ... }`

#### 16-A-7: Dockerfile hardening

**Touch-points:**

- `Dockerfile.api` — multiple changes:
  1. Add `.dockerignore` file (new): exclude `target/`, `workdir/`, `sites/`,
     `artifacts/`, `coverage/`, `*.whl`, `.git/`, `local-api-copy/`
  2. Update builder to `rust:1.87-bookworm` (tracks stable)
  3. Remove `COPY src/ ./src/` and `COPY templates/ ./templates/` — not needed
     to build the API binary
  4. Change `CMD` to `ENTRYPOINT ["/app/genomehubs-api"]` + `CMD []` so that
     `docker run ... /app/genomehubs-api --help` works without override
  5. Add `ENV ES_INTEGRATION_CONFIG=/app/config/es_integration.toml` so the
     container auto-discovers the mounted config without extra `-e` flags
  6. Add OCI image labels
  7. Fix health-check to use `http://127.0.0.1:3000` not `localhost:3000`
     (avoids IPv6 resolution delay)

- New file: `.dockerignore` in project root

---

### Phase 16-B: Rate limiting (~1 day)

#### 16-B-1: Per-IP rate limiting via `tower-governor`

`tower-governor` is a production-ready Axum-compatible rate-limiting layer
backed by an in-process governor (token-bucket algorithm). It requires no
external service and is the right choice for the v3 standalone deployment.

**New dependency:**

```toml
tower-governor = { version = "0.4", features = ["axum"] }
```

**Strategy:**

Apply two tiers of rate limits:

| Tier       | Endpoints                                                        | Limit              |
| ---------- | ---------------------------------------------------------------- | ------------------ |
| `bulk`     | POST `/count/batch`, `/search/batch`, `/report`                  | 10 req/min per IP  |
| `standard` | all other POST endpoints (`/count`, `/search`, `/summary`, etc.) | 60 req/min per IP  |
| `read`     | all GET endpoints                                                | 120 req/min per IP |

GET endpoints serving metadata (`/metadata/*`, `/status`) and the Swagger UI
assets are cheap; no limit or a very high limit is fine.

**New file:** `crates/genomehubs-api/src/middleware/rate_limit.rs`

```rust
// Provides:
//   pub fn bulk_layer() -> impl Layer<...>
//   pub fn standard_layer() -> impl Layer<...>
//   pub fn read_layer() -> impl Layer<...>
// Each returns a GovernorLayer configured for the tier.
```

**Wire-up:** Route groups in `main.rs` need to be split by tier. Axum supports
nested routers:

```rust
let bulk_router = Router::new()
    .route("/api/v3/count/batch", post(routes::count_batch::post_count_batch))
    .route("/api/v3/search/batch", post(routes::search_batch::post_search_batch))
    .route("/api/v3/report", post(routes::report::post_report))
    .layer(middleware::rate_limit::bulk_layer());

let standard_router = Router::new()
    .route("/api/v3/count", post(routes::count::post_count))
    .route("/api/v3/search", post(routes::search::post_search))
    .layer(middleware::rate_limit::standard_layer());

let app = Router::new()
    .merge(bulk_router)
    .merge(standard_router)
    // ... GET routes, no rate limiting
    .layer(Extension(state))
    .layer(middleware::security_headers::layer())
    .layer(DefaultBodyLimit::max(1_024 * 1_024));
```

#### 16-B-2: RFC 7807 error body on 429

`tower-governor` by default returns an empty 429. Override the error handler
to return a structured JSON body:

```json
{
  "type": "https://genomehubs.org/errors/rate-limit-exceeded",
  "title": "Too Many Requests",
  "status": 429,
  "detail": "Rate limit exceeded. Please slow down and retry after {retry_after} seconds.",
  "retry_after": 60
}
```

**Touch-points:**

- `crates/genomehubs-api/src/middleware/rate_limit.rs` — implement
  `GovernorConfigBuilder::error_handler` with the RFC 7807 body
- Ensure `Content-Type: application/problem+json` on 429 responses

#### 16-B-3: Rate limit configuration via `es_integration.toml`

Add optional TOML fields so operators can tune limits without recompiling:

```toml
[rate_limits]
bulk_rpm = 10
standard_rpm = 60
read_rpm = 120
```

Parse these in `main.rs` and pass to the layer constructors.

**Touch-points:**

- `main.rs` `EsConfig` struct — add `rate_limits: Option<RateLimitConfig>`
- `middleware/rate_limit.rs` — accept config struct in layer constructors
- `config/es_integration.toml` — document new section

---

### Phase 16-C: Redis result caching (~2 days)

#### 16-C-1: Redis connection management

Add `redis` (async, `tokio-comp` feature) as a dependency. Store a connection
pool in `AppState`.

**New dependency:**

```toml
redis = { version = "0.27", features = ["tokio-comp", "connection-manager"] }
```

**Touch-points:**

- `crates/genomehubs-api/Cargo.toml` — add redis
- `main.rs` `AppState` — add `redis: Option<redis::aio::ConnectionManager>`
- `main.rs` `EsConfig` — add `redis_url: Option<String>`
  (default: `redis://localhost:6379`; `None` disables caching)
- `main.rs` startup — try to connect; log warning and continue if Redis is
  unavailable (cache is optional, not required for startup)
- `config/es_integration.toml` — add `redis_url = "redis://redis:6379"`
  (commented by default; uncomment to enable)

#### 16-C-2: Cache key design — v3 namespace

**Key format:**

```
genomehubs:v3:{hub_name}:{endpoint}:{sha256_hex(canonical_request)}
```

Example:

```
genomehubs:v3:goat:search:a3f2b1...
genomehubs:v3:goat:count:7e91d0...
```

Prefix `genomehubs:v3:` is **never** used by the v2 API (which uses `genomehubs:v2:`
or no prefix). `{hub_name}` (e.g. `goat`, `boat`) further scopes keys when two
sites share a Redis instance.

The `canonical_request` is the **normalised** request body serialised to
canonical JSON (keys sorted, whitespace stripped) before hashing. This ensures
`{"query_yaml":"...","params_yaml":"..."}` and
`{"params_yaml":"...","query_yaml":"..."}` hit the same cache entry.

**New file:** `crates/genomehubs-api/src/cache.rs`

```rust
/// Build a Redis cache key for a v3 API endpoint.
pub fn make_key(hub_name: &str, endpoint: &str, body: &serde_json::Value) -> String
/// Try to get a cached value. Returns None on miss or Redis error.
pub async fn get(conn: &mut redis::aio::ConnectionManager, key: &str) -> Option<serde_json::Value>
/// Store a value. Uses configurable TTL (default 5 minutes).
pub async fn set(conn: &mut redis::aio::ConnectionManager, key: &str, value: &serde_json::Value, ttl_secs: u64)
```

`make_key` implementation sketch:

```rust
use sha2::{Digest, Sha256};

pub fn make_key(hub_name: &str, endpoint: &str, body: &serde_json::Value) -> String {
    let canonical = canonicalise(body);           // sort keys, compact JSON
    let hash = hex::encode(Sha256::digest(canonical.as_bytes()));
    format!("genomehubs:v3:{hub_name}:{endpoint}:{hash}")
}
```

New dependencies for hashing:

```toml
sha2 = "0.10"
hex = "0.4"
```

#### 16-C-3: Cache TTL configuration

Default TTLs per endpoint category (short enough to reflect data updates,
long enough to cut ES load):

| Category                   | Default TTL                               |
| -------------------------- | ----------------------------------------- |
| `/count`, `/count/batch`   | 300 s (5 min)                             |
| `/search`, `/search/batch` | 300 s (5 min)                             |
| `/report`, `/summary`      | 600 s (10 min)                            |
| `/record`                  | 600 s (10 min)                            |
| `/lookup`                  | 60 s (1 min — user-facing autocomplete)   |
| `/metadata/*`              | 3600 s (1 hour — changes only on reindex) |

These are configurable in `es_integration.toml`:

```toml
[cache_ttl]
search_secs    = 300
count_secs     = 300
report_secs    = 600
record_secs    = 600
lookup_secs    = 60
metadata_secs  = 3600
```

#### 16-C-4: Cache integration into handler chain

**Pattern for every cacheable endpoint:**

```rust
// In each POST handler:
if let Some(ref mut redis) = state.redis_conn() {
    let key = cache::make_key(&state.hub_name, "search", &canonical_body);
    if let Some(cached) = cache::get(redis, &key).await {
        return Json(cached.into());   // fast path
    }
}
// ... execute ES query ...
if let Some(ref mut redis) = state.redis_conn() {
    cache::set(redis, &key, &response_json, ttl).await;
}
```

**Touch-points (one per endpoint):**

- `routes/count.rs`
- `routes/count_batch.rs`
- `routes/search.rs`
- `routes/search_batch.rs`
- `routes/report.rs`
- `routes/record.rs`
- `routes/lookup.rs`
- `routes/summary.rs`
- `es_metadata.rs` — cache metadata responses (longest TTL)

**Helper on AppState:**

```rust
impl AppState {
    /// Cheaply clone the connection manager handle if Redis is configured.
    pub fn redis_conn(&self) -> Option<redis::aio::ConnectionManager> {
        self.redis.clone()
    }
}
```

`ConnectionManager` internally holds an `Arc`; `.clone()` is cheap.

#### 16-C-5: Cache-Control response headers

For clients and CDNs, add `Cache-Control` headers to cacheable GET and POST
responses:

```
Cache-Control: public, max-age=300, stale-while-revalidate=60
```

These values should mirror the Redis TTL for each category so that edge caches
and browsers apply consistent freshness windows.

Add to the security-headers middleware (16-A-1) as a per-endpoint override, or
as a separate layer that reads a custom response extension.

---

## 4. File touch-points summary

| File                                                       | Phase    | What changes                                                                     |
| ---------------------------------------------------------- | -------- | -------------------------------------------------------------------------------- |
| `crates/genomehubs-api/src/main.rs`                        | 16-A     | Timeouts, bind addr, logging, OpenAPI write guard, rate-limit wiring, Redis init |
| `crates/genomehubs-api/src/middleware/mod.rs`              | 16-A     | New module                                                                       |
| `crates/genomehubs-api/src/middleware/security_headers.rs` | 16-A-1   | New                                                                              |
| `crates/genomehubs-api/src/middleware/rate_limit.rs`       | 16-B-1   | New                                                                              |
| `crates/genomehubs-api/src/cache.rs`                       | 16-C-2   | New                                                                              |
| `crates/genomehubs-api/src/es_client.rs`                   | 16-A-3   | Add timeout params                                                               |
| `crates/genomehubs-api/src/routes/count.rs`                | 16-C-4   | Cache read/write                                                                 |
| `crates/genomehubs-api/src/routes/count_batch.rs`          | 16-C-4   | Cache read/write                                                                 |
| `crates/genomehubs-api/src/routes/search.rs`               | 16-C-4   | Cache read/write                                                                 |
| `crates/genomehubs-api/src/routes/search_batch.rs`         | 16-C-4   | Cache read/write                                                                 |
| `crates/genomehubs-api/src/routes/report.rs`               | 16-C-4   | Cache read/write                                                                 |
| `crates/genomehubs-api/src/routes/record.rs`               | 16-C-4   | Cache read/write                                                                 |
| `crates/genomehubs-api/src/routes/lookup.rs`               | 16-C-4   | Cache read/write                                                                 |
| `crates/genomehubs-api/src/routes/summary.rs`              | 16-C-4   | Cache read/write                                                                 |
| `crates/genomehubs-api/src/es_metadata.rs`                 | 16-C-4   | Cache metadata response                                                          |
| `crates/genomehubs-api/Cargo.toml`                         | 16-A/B/C | Add tower, tower-governor, redis, sha2, hex                                      |
| `config/es_integration.toml`                               | 16-A/B/C | Document new config fields                                                       |
| `Dockerfile.api`                                           | 16-A-7   | Hardening, ENTRYPOINT, ENV, labels                                               |
| `.dockerignore`                                            | 16-A-7   | New                                                                              |

---

## 5. Dependency additions

```toml
# Security / middleware
tower           = { version = "0.4", features = ["util"] }
http            = "1.0"
tower-governor  = { version = "0.4", features = ["axum"] }
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Redis caching
redis  = { version = "0.27", features = ["tokio-comp", "connection-manager"] }
sha2   = "0.10"
hex    = "0.4"
```

No dependency requires a new `crates/` workspace member. All are direct
dependencies of `genomehubs-api`.

---

## 6. Redis key clash analysis

| Version          | Key prefix                    | Example                            |
| ---------------- | ----------------------------- | ---------------------------------- |
| v2 (Python/Node) | `genomehubs:v2:` or bare hash | `genomehubs:v2:goat:search:abc123` |
| v3 (this crate)  | `genomehubs:v3:`              | `genomehubs:v3:goat:search:a3f2b1` |

Even if both versions hash the same request body identically, the `v2`/`v3`
segment guarantees no overlap. The `{hub_name}` segment additionally isolates
GoaT from BoaT if they share a Redis instance.

**Verification step (must complete before enabling shared Redis):**

```bash
redis-cli keys "genomehubs:*" | head -20
# Confirm no keys start with "genomehubs:v3:"
```

---

## 7. Suggested implementation order

1. **16-A-7** (Dockerfile / `.dockerignore`) — quick win, improves CI build times immediately
2. **16-A-1 to 16-A-6** — security hardening; low risk, no functional change
3. **16-B** — rate limiting; wire up and test with `wrk` or `hey`
4. **16-C-1** — Redis connection (no caching yet; just connectivity)
5. **16-C-2 to 16-C-3** — cache key design + TTL config
6. **16-C-4** — integrate into handlers one endpoint at a time (start with `/search`)
7. **16-C-5** — Cache-Control headers

---

## 8. Testing plan

### Unit tests (in-process, no Docker)

- `cache::make_key` returns identical keys for equivalent bodies with different
  key ordering (proptest: shuffle keys, assert same hash)
- `cache::make_key` returns `genomehubs:v3:` prefix always
- Security headers middleware attaches all required headers to every response
- Rate limiter returns HTTP 429 with `application/problem+json` body and
  `Retry-After` header after limit exceeded

### Integration tests (require running Redis)

- Cache miss → ES hit → cache set → second request → cache hit (verify ES not called twice)
- TTL expiry: set with TTL=1, sleep 2s, assert miss
- v2 and v3 keys do not collide (write v2-format key manually, assert v3 key
  lookup returns miss)

### Load test

```bash
# Install hey: brew install hey
hey -n 100 -c 10 -m POST -H "Content-Type: application/json" \
    -d '{"query_yaml":"index: taxon\nquery: tax_tree(Mammalia)\n","params_yaml":"size: 0\n"}' \
    http://localhost:3000/api/v3/count
# Expect: first N requests hit ES; subsequent identical requests return from cache
# Expect: requests 61-100 from same IP within 1 minute return 429
```

---

## 9. Open questions for operator before implementation

1. **Redis shared vs. dedicated** — Is the v2 Redis instance accessible from
   the v3 container network, or should v3 run its own Redis sidecar?
2. **Rate-limit IP extraction** — Is the API behind a reverse proxy (nginx,
   Caddy, Traefik)? If so, `X-Forwarded-For` must be trusted and parsed for
   the real client IP. This requires `tower-governor` to be configured with
   a custom key extractor.
3. **Swagger UI in production** — Should `/swagger-ui` be disabled or
   protected (e.g. `ENABLE_SWAGGER_UI=1` env guard)?
4. **TLS termination** — Is TLS terminated at the reverse proxy (most common)?
   If so, `Strict-Transport-Security` in the middleware is appropriate. If
   the binary handles TLS directly, `rustls` + `axum-server` would be needed.
5. **Cache invalidation on reindex** — When a new GoaT index is published,
   all cached search results become stale. Strategy: flush `genomehubs:v3:goat:*`
   keys from Redis as part of the reindex pipeline, or use a very short TTL
   (60 s) that self-heals.
