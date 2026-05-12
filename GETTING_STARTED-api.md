# Getting Started: API Setup and Deployment

This guide covers setting up and running the **genomehubs-api** (Rust/Axum backend) for local development, testing, or deployment.

## Prerequisites

- **Elasticsearch** running and accessible (required for all methods)
- **Rust 1.81+** (for direct build)
- **Docker** (for containerized deployment)
- **curl** or similar for testing

## Quick Start: Docker (Recommended)

### 1. Run with Default Configuration

The simplest way to get started is to run the pre-built Docker image:

```bash
docker run -d \
  -p 3000:3000 \
  -e ES_INTEGRATION_CONFIG=/app/config/es_integration.toml \
  genomehubs/cli-generator-api:develop
```

The API will start on `http://localhost:3000` and attempt to connect to a local Elasticsearch at `http://localhost:9200` with default hub settings.

### Pulling from GHCR (GitHub Container Registry)

You can pull the official image from GitHub Container Registry. The repository image used by CI is:

`ghcr.io/genomehubs/genomehubs-api-v3:develop`

Pull and run:

```bash
docker pull ghcr.io/genomehubs/genomehubs-api-v3:develop
docker run -d \
  -p 3000:3000 \
  -e ES_INTEGRATION_CONFIG=/app/config/es_integration.toml \
  ghcr.io/genomehubs/genomehubs-api-v3:develop
```

### 2. Run with Custom Configuration via TOML

Create an `es_integration.toml` file with your settings:

```toml
base_url = "http://elasticsearch:9200"
default_result = "taxon"
default_taxonomy = "ncbi"
default_version = "2021.10.15"
hub_name = "goat"
index_separator = "--"
```

Mount the config file into the container:

```bash
docker run -d \
  -p 3000:3000 \
  -v /path/to/es_integration.toml:/app/config/es_integration.toml \
  genomehubs/cli-generator-api:develop
```

### 3. Run with Environment Variables

Override configuration via environment variables:

```bash
docker run -d \
  -p 3000:3000 \
  -e ES_BASE_URL="http://elasticsearch:9200" \
  -e HUB_NAME="my-hub" \
  -e DEFAULT_TAXONOMY="ncbi" \
  -e DEFAULT_RESULT="taxon" \
  genomehubs/cli-generator-api:develop
```

### 4. Docker Compose Setup

For a complete stack with Elasticsearch:

```yaml
version: "3.8"
services:
  elasticsearch:
    image: docker.elastic.co/elasticsearch/elasticsearch:8.10.0
    environment:
      - discovery.type=single-node
      - "ES_JAVA_OPTS=-Xms512m -Xmx512m"
      - xpack.security.enabled=false
    ports:
      - "9200:9200"
    volumes:
      - es-data:/usr/share/elasticsearch/data

  api:
    image: genomehubs/cli-generator-api:develop
    ports:
      - "3000:3000"
    depends_on:
      - elasticsearch
    environment:
      - ES_BASE_URL=http://elasticsearch:9200
      - HUB_NAME=goat
      - DEFAULT_TAXONOMY=ncbi
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/api/v3/status"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 5s

volumes:
  es-data:
```

Start with:

```bash
docker-compose up -d
```

## Direct Build from Source

### Prerequisites

- Rust 1.81+ (install from [rustup.rs](https://rustup.rs/))
- Cargo

### Build Steps

1. **Clone the repository:**

```bash
git clone https://github.com/genomehubs/cli-generator.git
cd cli-generator
```

2. **Build the API binary:**

```bash
cargo build -p genomehubs-api --release
```

The binary will be at `target/release/genomehubs-api`.

3. **Create configuration:**

Create `config/es_integration.toml` in the project root:

```toml
base_url = "http://localhost:9200"
default_result = "taxon"
default_taxonomy = "ncbi"
default_version = "2021.10.15"
hub_name = "goat"
index_separator = "--"
```

4. **Run the API:**

```bash
./target/release/genomehubs-api
```

The API will listen on `http://0.0.0.0:3000`.

## Configuration

### Configuration File Format (`es_integration.toml`)

```toml
# Required: Elasticsearch base URL (no trailing slash)
base_url = "http://localhost:9200"

# Optional: defaults shown below
default_result = "taxon"
default_taxonomy = "ncbi"
default_version = "2021.10.15"
hub_name = "goat"
index_separator = "--"
```

### Configuration Priority

The API resolves configuration in this order (first match wins):

1. **Environment variable**: `ES_INTEGRATION_CONFIG=/path/to/custom.toml`
2. **File search**: Walks up directory tree from current working directory looking for `config/es_integration.toml`
3. **Fallback**: Uses `config/es_integration.toml.example` if no config found
4. **Defaults**: Built-in defaults if all above fail

### Environment Variables

You can override config file values with environment variables:

```bash
export ES_BASE_URL="http://elasticsearch:9200"
export DEFAULT_RESULT="taxon"
export DEFAULT_TAXONOMY="ncbi"
export DEFAULT_VERSION="2021.10.15"
export HUB_NAME="my-hub"
export INDEX_SEPARATOR="--"
```

Then run:

```bash
./target/release/genomehubs-api
```

## Running the API Locally for Development

The fastest way to see the Swagger UI with the current examples is a dev build
from the project root.  No extra flags are needed — the examples are compiled
into the binary.

### 1. Ensure Elasticsearch is running

The API will refuse to start if it cannot populate the metadata cache from ES.
If you don't have a running ES instance, start one with Docker:

```bash
docker run -d --name es-local \
  -p 9200:9200 \
  -e "discovery.type=single-node" \
  -e "xpack.security.enabled=false" \
  docker.elastic.co/elasticsearch/elasticsearch:8.10.0
```

### 2. Configure `config/es_integration.toml`

If the file does not exist yet, copy the example:

```bash
cp config/es_integration.toml.example config/es_integration.toml
```

Edit it to match your instance — at minimum set `base_url`, `hub_name`, and
`default_version`:

```toml
base_url        = "http://localhost:9200"
hub_name        = "goat"
default_version = "2021.10.15"
index_separator = "--"
default_taxonomy = "ncbi"
```

For docker-compose use with the bundled ES image, `base_url` should be
`http://localhost:9200` (default).

### 3. Start the API

```bash
# From the project root — auto-discovers config/es_integration.toml
cargo run -p genomehubs-api
```

Or, to use a different config file:

```bash
ES_INTEGRATION_CONFIG=/path/to/my-site.toml cargo run -p genomehubs-api
```

The API will log:

```
INFO metadata cache populated
INFO Listening on 127.0.0.1:3000
```

### 4. Open Swagger UI

```
http://localhost:3000/swagger-ui/
```

Endpoints are grouped into four tags matching the v2 documentation layout:

| Tag | Endpoints |
|-----|-----------|
| **Data** | `/count`, `/count/batch`, `/search`, `/search/batch`, `/record`, `/report`, `/lookup`, `/summary` |
| **Metadata** | `/metadata`, `/metadata/fields`, `/metadata/indices`, `/metadata/ranks`, `/metadata/taxonomies` |
| **External** | `/phylopic`, `/phylopic/batch` |
| **Status** | `/status` |

Request body examples (e.g. "Mammalia with genome size") are shown inside the
"Try it out" panel for POST endpoints.

---

## Swagger Customisation (Runtime)

The Swagger UI title, description, contact block, and request-body examples
are **loaded at runtime** from a YAML file.  This means any Docker deployment
can be customised by mounting a single file — no rebuild required.

### Enabling customisation

Add one line to `es_integration.toml` (or the mounted override):

```toml
swagger_examples = "config/swagger-examples-goat.yaml"
```

The API reads this file at startup.  Restart the API to pick up edits.

> Relative paths are resolved from the process working directory.  Inside a
> Docker container, use an absolute path or mount the file to a predictable
> location.

### YAML file structure

`config/swagger-examples-goat.yaml` is the canonical example for GoaT.  The
file has two top-level keys:

#### `info` — API info block override

```yaml
info:
  title: "GoaT API"
  description: |
    **Genomes on a Tree** API description (Markdown supported).
  contact:
    name: "GoaT"
    url: "https://goat.genomehubs.org"
    email: "goat@genomehubs.org"
  license:
    name: "MIT License"
    url: "https://github.com/genomehubs/genomehubs/blob/main/LICENSE"
```

All fields are optional.  Omitted fields keep their compiled defaults.

#### `examples` — request-body examples

```yaml
examples:
  - path: "/api/v3/count"
    method: post
    name: mammalia_species_count
    summary: "Count Mammalia taxa with a genome size estimate"
    value:
      query_yaml: "index: taxon\nquery: tax_tree(Mammalia) AND genome_size\n"
      params_yaml: "size: 0\ninclude_estimates: true\ntaxonomy: ncbi\n"

  - path: "/api/v3/search"
    method: post
    name: mammalia_genome_size
    summary: "Search Mammalia taxa with genome size, sorted descending"
    value:
      query_yaml: "index: taxon\nquery: tax_tree(Mammalia) AND genome_size\n"
      params_yaml: "size: 10\nfields: genome_size,scientific_name\nsort_by: genome_size\nsort_order: desc\ninclude_estimates: true\ntaxonomy: ncbi\n"
```

* `path` — API path exactly as it appears in the OpenAPI spec.
* `method` — HTTP method in lowercase (`post`, `get`, …).
* `name` — key in the OpenAPI `examples` map (no spaces).
* `summary` — one-line label shown in the Swagger UI dropdown.
* `value` — the example request body as a YAML mapping.

If the YAML provides **any** examples for a given `path`+`method` pair, those
examples **replace** the compile-time defaults for that endpoint.  Endpoints
not mentioned in the YAML keep their compiled defaults.

### Docker deployment

Mount the two config files at container start and set the path:

```sh
docker run \
  -v /host/config/es_integration.toml:/app/config/es_integration.toml \
  -v /host/config/swagger-examples-goat.yaml:/app/config/swagger-examples-goat.yaml \
  genomehubs-api
```

with `es_integration.toml` containing:

```toml
swagger_examples = "/app/config/swagger-examples-goat.yaml"
```

### Adding a new site

1. Copy `config/swagger-examples-goat.yaml` to
   `config/swagger-examples-<site>.yaml`.
2. Edit the `info` block and `examples` list for the new site.
3. Mount it alongside `es_integration.toml` and point `swagger_examples` at it.

The compile-time examples in the route source files (`routes/count.rs` etc.)
remain as generic fallbacks for deployments that do not mount a customisation
file.

---

## Testing the API

### Health Check

```bash
curl http://localhost:3000/api/v3/status
```

### API Documentation

Once running, access the Swagger UI documentation at:

```
http://localhost:3000/swagger-ui/
```

### Example Queries

**Get taxonomies:**

```bash
curl http://localhost:3000/api/v3/taxonomies
```

**Get indices:**

```bash
curl http://localhost:3000/api/v3/metadata/indices
```

**Search:**

```bash
curl -X POST http://localhost:3000/api/v3/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "sapiens",
    "limit": 10
  }'
```

## Logging and Debugging

### View Logs

Logs are printed to stdout with structured tracing. Set the `RUST_LOG` environment variable to control verbosity:

```bash
# In Docker:
docker run -e RUST_LOG=info genomehubs/cli-generator-api:develop

# Direct build:
RUST_LOG=info ./target/release/genomehubs-api
```

Log levels: `trace`, `debug`, `info`, `warn`, `error`

### Example: Full debug logging

```bash
docker run -e RUST_LOG=debug genomehubs/cli-generator-api:develop
```

## Deployment

### Kubernetes

Example manifest:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: genomehubs-api
spec:
  replicas: 2
  selector:
    matchLabels:
      app: genomehubs-api
  template:
    metadata:
      labels:
        app: genomehubs-api
    spec:
      containers:
        - name: api
          image: genomehubs/cli-generator-api:develop
          ports:
            - containerPort: 3000
          env:
            - name: ES_BASE_URL
              value: "http://elasticsearch:9200"
            - name: HUB_NAME
              value: "goat"
          livenessProbe:
            httpGet:
              path: /api/v3/status
              port: 3000
            initialDelaySeconds: 10
            periodSeconds: 30
          readinessProbe:
            httpGet:
              path: /api/v3/status
              port: 3000
            initialDelaySeconds: 5
            periodSeconds: 10
          resources:
            requests:
              memory: "256Mi"
              cpu: "250m"
            limits:
              memory: "512Mi"
              cpu: "500m"
---
apiVersion: v1
kind: Service
metadata:
  name: genomehubs-api
spec:
  selector:
    app: genomehubs-api
  ports:
    - protocol: TCP
      port: 80
      targetPort: 3000
  type: LoadBalancer
```

### Environment-specific Configuration

#### Development

```bash
# Mount a local config file for easy iteration
docker run -v $(pwd)/config:/app/config \
  -p 3000:3000 \
  genomehubs/cli-generator-api:develop
```

#### Staging

```bash
docker run \
  -e ES_BASE_URL="http://elasticsearch-staging:9200" \
  -e HUB_NAME="staging-hub" \
  -p 3000:3000 \
  genomehubs/cli-generator-api:develop
```

#### Production

Use a config file or secrets management system (e.g., Kubernetes Secrets):

```bash
# Via mounted secret
docker run \
  -v /run/secrets/es_config:/app/config/es_integration.toml:ro \
  -p 3000:3000 \
  genomehubs/cli-generator-api:develop
```

## Troubleshooting

### API won't start: "Failed to populate metadata cache"

**Cause**: Elasticsearch is not reachable or not running.

**Solution**:

1. Verify Elasticsearch is running: `curl http://your-es-url:9200`
2. Check `ES_BASE_URL` or `base_url` in config
3. Ensure network connectivity between API container and Elasticsearch

### "Index not found" errors

**Cause**: The configured hub indices don't exist in Elasticsearch.

**Solution**:

1. Verify indices exist: `curl http://your-es-url:9200/_cat/indices`
2. Check `hub_name`, `default_taxonomy`, and `index_separator` values
3. Ensure indices match the naming pattern: `{separator}{taxonomy}{separator}{hub}{separator}{version}`

### High memory usage

**Cause**: Large metadata cache (many indices/taxonomies).

**Solution**:

1. Increase container memory limits
2. Consider filtering indices (if supported by your setup)
3. Monitor cache population logs

### Slow startup

**Cause**: Large Elasticsearch cluster or slow network.

**Solution**:

1. Check Elasticsearch response times
2. Increase startup timeout in health checks
3. Monitor logs with `RUST_LOG=debug`

## API Endpoints

| Method | Endpoint                      | Description                   |
| ------ | ----------------------------- | ----------------------------- |
| GET    | `/api/v3/status`              | Health check and API version  |
| GET    | `/api/v3/metadata`            | All metadata (one round-trip) |
| GET    | `/api/v3/metadata/indices`    | List available indices        |
| GET    | `/api/v3/metadata/taxonomies` | List available taxonomies     |
| GET    | `/api/v3/metadata/ranks`      | List taxonomic ranks          |
| GET    | `/api/v3/metadata/fields`     | List available result fields  |
| POST   | `/api/v3/search`              | Search records                |
| POST   | `/api/v3/search/batch`        | Batch search                  |
| POST   | `/api/v3/count`               | Count records                 |
| POST   | `/api/v3/count/batch`         | Batch count                   |
| GET    | `/api/v3/lookup`              | Quick lookup by name/ID       |
| GET    | `/api/v3/record`              | Get specific record           |
| POST   | `/api/v3/report`              | Generate reports              |
| GET    | `/api/v3/phylopic`            | PhyloPic image for a taxon    |
| POST   | `/api/v3/phylopic/batch`      | Batch PhyloPic lookup         |
| GET    | `/swagger-ui/`                | API documentation (Swagger)   |

For detailed endpoint documentation, see the Swagger UI at `http://localhost:3000/swagger-ui/`.

## Building Your Own Docker Image

To build the Docker image locally:

```bash
docker build -f Dockerfile.api -t my-genomehubs-api:latest .
docker run -p 3000:3000 my-genomehubs-api:latest
```

## Additional Resources

- [API Source Code](../crates/genomehubs-api/)
- [Elasticsearch Documentation](https://www.elastic.co/guide/en/elasticsearch/reference/current/index.html)
- [Axum Web Framework](https://github.com/tokio-rs/axum)
- [Tokio Async Runtime](https://tokio.rs/)

## Support

For issues, questions, or contributions, see the main [README.md](../README.md) and [CONTRIBUTING.md](../CONTRIBUTING.md).
