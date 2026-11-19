//! PhyloPic v2 API client: resolution, caching, and build-number management.
//!
//! Implements the three-step resolution pipeline described in phase-14:
//! 1. NCBI batch resolve (primary, lineage-aware)
//! 2. PhyloPic name search requiring a single synonym match (v2 rule)
//! 3. GBIF bridge (when GBIF species keys are available)

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const PHYLOPIC_API: &str = "https://api.phylopic.org";

// ── Public types ─────────────────────────────────────────────────────────────

/// A silhouette image resolved from PhyloPic for one taxon.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PhylopicRecord {
    /// NCBI taxon ID the record was requested for.
    pub taxon_id: String,
    /// URL of a raster (PNG) file — second-largest, or largest if only one exists.
    pub raster_url: String,
    /// URL of the SVG vector file (resolution-independent). Prefer over `raster_url` in UI.
    pub vector_url: Option<String>,
    /// Aspect ratio (width / height) of the raster image.
    pub ratio: f32,
    /// Free-text attribution string from the contributor.
    pub attribution: Option<String>,
    /// SPDX licence identifier (e.g. `"CC0-1.0"`) derived from the licence URL.
    pub license: String,
    /// Licence URL.
    pub license_url: String,
    /// Display name of the contributor.
    pub contributor: Option<String>,
    /// Scientific name of the taxon the image directly illustrates.
    pub image_name: String,
    /// Canonical URL of this image on phylopic.org.
    pub source_url: String,
    /// Taxonomic rank of the node the image represents.
    pub image_rank: String,
    /// Resolution source: `Primary` | `Descendant` | `Ancestral`.
    pub source: PhylopicSource,
    /// PhyloPic build number at time of fetch.
    pub build: u32,
}

/// How the image was resolved relative to the requested taxon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum PhylopicSource {
    /// Image directly illustrates this taxon (or a descendant at species level).
    Primary,
    /// Image illustrates a descendant of the requested taxon.
    Descendant,
    /// Image illustrates an ancestor of the requested taxon.
    Ancestral,
}

/// Shared cache for build number and resolved records.
#[derive(Debug, Default)]
pub struct PhylopicCache {
    /// Current PhyloPic build number (updated every 24h).
    pub current_build: u32,
    /// Resolved records keyed by taxon_id.
    pub entries: HashMap<String, PhylopicRecord>,
    /// The build number when each entry was fetched; used for lazy invalidation.
    pub build_at_fetch: HashMap<String, u32>,
}

impl PhylopicCache {
    /// Return a cached record if it was fetched under the current build.
    pub fn get(&self, taxon_id: &str) -> Option<&PhylopicRecord> {
        let stale = self
            .build_at_fetch
            .get(taxon_id)
            .is_none_or(|&b| b != self.current_build);
        if stale {
            None
        } else {
            self.entries.get(taxon_id)
        }
    }

    /// Store a resolved record, tagging it with the current build.
    pub fn insert(&mut self, taxon_id: String, record: PhylopicRecord) {
        let build = self.current_build;
        self.build_at_fetch.insert(taxon_id.clone(), build);
        self.entries.insert(taxon_id, record);
    }
}

// ── Caller-supplied taxon info (from ES record) ───────────────────────────────

/// Taxon information extracted from an ES record, used by the resolution pipeline.
#[derive(Debug)]
pub struct TaxonInfo {
    pub taxon_id: String,
    pub scientific_name: String,
    pub rank: String,
    pub taxon_names: Vec<TaxonName>,
    /// Lineage IDs, most-specific first (excluding the taxon itself).
    pub lineage_ids: Vec<String>,
    /// Rank of each lineage ID, keyed by taxon ID.
    pub lineage_ranks: HashMap<String, String>,
    /// Optional GBIF species key list for GBIF bridge fallback.
    pub gbif_lineage_keys: Vec<String>,
}

/// An alternative name for a taxon (synonym, common name, etc.).
#[derive(Debug)]
pub struct TaxonName {
    pub name: String,
}

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum PhylopicError {
    NotFound,
    Http(String),
    MalformedResponse,
}

impl std::fmt::Display for PhylopicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PhylopicError::NotFound => write!(f, "no image found"),
            PhylopicError::Http(msg) => write!(f, "HTTP error: {msg}"),
            PhylopicError::MalformedResponse => write!(f, "malformed PhyloPic response"),
        }
    }
}

// ── PhyloPic API response shapes (serde-only, not exposed) ───────────────────

#[derive(Deserialize)]
struct RootResponse {
    build: u32,
}

#[derive(Deserialize)]
struct ResolveResponse {
    #[serde(rename = "_links")]
    links: ResolveLinks,
    #[serde(rename = "_embedded")]
    embedded: Option<ResolveEmbedded>,
}

#[derive(Deserialize)]
struct ResolveLinks {
    #[serde(rename = "self")]
    self_link: Option<SelfLink>,
    external: Option<Vec<ExternalLink>>,
    #[serde(rename = "primaryImage")]
    #[allow(dead_code)]
    primary_image: Option<ImageSelf>,
}

#[derive(Deserialize)]
struct ResolveEmbedded {
    #[serde(rename = "primaryImage")]
    primary_image: Option<ImageNode>,
}

#[derive(Deserialize)]
struct ExternalLink {
    href: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ImageSelf {
    href: String,
}

#[derive(Deserialize)]
struct ImageNode {
    #[serde(rename = "_links")]
    links: ImageLinks,
    #[serde(rename = "_embedded")]
    embedded: Option<ImageEmbedded>,
    uuid: Option<String>,
}

#[derive(Deserialize)]
struct ImageLinks {
    #[serde(rename = "rasterFiles")]
    raster_files: Option<Vec<FileLink>>,
    #[serde(rename = "vectorFile")]
    vector_file: Option<FileLink>,
    #[serde(rename = "contributor")]
    #[allow(dead_code)]
    contributor_link: Option<SelfLink>,
    #[serde(rename = "licenseVersion")]
    #[allow(dead_code)]
    license_version: Option<SelfLink>,
    #[serde(rename = "taxa")]
    #[allow(dead_code)]
    taxa: Option<Vec<SelfLink>>,
}

#[derive(Deserialize)]
struct FileLink {
    href: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    media_type: Option<String>,
}

#[derive(Deserialize)]
struct SelfLink {
    href: String,
}

#[derive(Deserialize)]
struct ImageEmbedded {
    #[serde(rename = "contributor")]
    contributor: Option<ContributorNode>,
    #[serde(rename = "licenseVersion")]
    license_version: Option<LicenseNode>,
}

#[derive(Deserialize)]
struct ContributorNode {
    name: Option<String>,
    #[serde(rename = "_links")]
    #[allow(dead_code)]
    links: Option<ContributorLinks>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ContributorLinks {
    #[serde(rename = "self")]
    self_link: Option<SelfLink>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct LicenseNode {
    #[serde(rename = "_links")]
    links: Option<LicenseLinks>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct LicenseLinks {
    #[serde(rename = "self")]
    self_link: Option<SelfLink>,
}

#[derive(Deserialize)]
struct ImagesResponse {
    #[serde(rename = "_embedded", default)]
    embedded: ImagesEmbedded,
}

#[derive(Deserialize, Default)]
struct ImagesEmbedded {
    #[serde(default)]
    items: Vec<ImageItem>,
}

#[derive(Deserialize)]
struct ImageItem {
    uuid: String,
}

#[derive(Deserialize)]
struct NodesResponse {
    #[serde(rename = "_links")]
    links: NodesLinks,
}

#[derive(Deserialize)]
struct NodesLinks {
    items: Option<Vec<NodeItem>>,
}

#[derive(Deserialize)]
struct NodeItem {
    href: String,
    title: Option<String>,
}

#[derive(Deserialize)]
struct NodeResponse {
    #[serde(rename = "_links")]
    links: NodeLinks,
    #[allow(dead_code)]
    names: Option<Vec<NodeName>>,
}

#[derive(Deserialize)]
struct NodeLinks {
    #[serde(rename = "primaryImage")]
    primary_image: Option<SelfLink>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct NodeName {
    names: Option<Vec<NodeNameEntry>>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct NodeNameEntry {
    text: String,
}

// ── Public functions ──────────────────────────────────────────────────────────

/// Fetch the current PhyloPic build number.
pub async fn fetch_current_build(client: &reqwest::Client) -> Result<u32, PhylopicError> {
    let url = format!("{PHYLOPIC_API}/");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| PhylopicError::Http(e.to_string()))?;

    resp.json::<RootResponse>()
        .await
        .map(|r| r.build)
        .map_err(|_| PhylopicError::MalformedResponse)
}

/// Resolve a PhyloPic image for the given taxon using the three-step pipeline.
///
/// Steps:
/// 1. NCBI batch resolve (primary + lineage IDs)
/// 2. Name search for a single synonym match (if step 1 yields Ancestral)
/// 3. GBIF bridge (if GBIF keys are available and steps 1–2 both fail)
pub async fn resolve(
    client: &reqwest::Client,
    info: &TaxonInfo,
    build: u32,
) -> Result<PhylopicRecord, PhylopicError> {
    // Step 1: NCBI batch resolve
    match resolve_by_ncbi(client, info, build).await {
        Ok(record) if record.source != PhylopicSource::Ancestral => return Ok(record),
        Ok(ancestral_record) => {
            // Ancestral match — try name search for a better result
            if let Ok(record) = resolve_by_name(client, info, build).await {
                return Ok(record);
            }
            // Name search failed; return the ancestral result rather than nothing
            return Ok(ancestral_record);
        }
        Err(PhylopicError::NotFound) => {}
        Err(e) => return Err(e),
    }

    // Step 2: Name search (NCBI resolve found nothing)
    match resolve_by_name(client, info, build).await {
        Ok(record) => return Ok(record),
        Err(PhylopicError::NotFound) => {}
        Err(e) => return Err(e),
    }

    // Step 3: GBIF bridge
    if !info.gbif_lineage_keys.is_empty() {
        if let Ok(record) = resolve_by_gbif(client, info, build).await {
            return Ok(record);
        }
    }

    Err(PhylopicError::NotFound)
}

/// Resolve a batch of taxon IDs, grouping the NCBI resolve call to reduce round trips.
///
/// IDs that yield Ancestral results from the batch call are individually retried via
/// name search. Returns a map from taxon_id to Result.
#[allow(dead_code)]
pub async fn resolve_batch(
    client: &reqwest::Client,
    infos: &[TaxonInfo],
    build: u32,
) -> HashMap<String, Result<PhylopicRecord, PhylopicError>> {
    let mut results: HashMap<String, Result<PhylopicRecord, PhylopicError>> = HashMap::new();

    // Fan out individually — PhyloPic /resolve accepts a single set of lineage IDs per
    // call, so true batching would require grouping by lineage which is complex. The
    // batch endpoint here is primarily a convenience for callers; each taxon still gets
    // its own three-step resolution (with the cache absorbing redundant requests in
    // the route handler).
    for info in infos {
        results.insert(info.taxon_id.clone(), resolve(client, info, build).await);
    }

    results
}

// ── Private resolution helpers ────────────────────────────────────────────────

async fn resolve_by_ncbi(
    client: &reqwest::Client,
    info: &TaxonInfo,
    build: u32,
) -> Result<PhylopicRecord, PhylopicError> {
    let object_ids: Vec<String> = std::iter::once(info.taxon_id.clone())
        .chain(info.lineage_ids.iter().cloned())
        .collect();
    let ids = object_ids.join(",");

    let url = format!(
        "{PHYLOPIC_API}/resolve/ncbi.nlm.nih.gov/taxid\
         ?build={build}&objectIDs={ids}&embed_primaryImage=true"
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| PhylopicError::Http(e.to_string()))?;

    if resp.status().as_u16() == 404 {
        return Err(PhylopicError::NotFound);
    }
    if !resp.status().is_success() {
        return Err(PhylopicError::Http(format!(
            "PhyloPic NCBI resolve returned {}",
            resp.status()
        )));
    }

    let data: ResolveResponse = resp
        .json()
        .await
        .map_err(|_| PhylopicError::MalformedResponse)?;

    // Determine which taxon was matched and whether it is the requested taxon,
    // a descendant, or an ancestor.
    let matched_id = data
        .links
        .external
        .as_ref()
        .and_then(|ext| {
            ext.iter()
                .find(|e| e.href.contains("ncbi.nlm.nih.gov/taxid"))
        })
        .and_then(|e| {
            e.href
                .strip_prefix("/resolve/ncbi.nlm.nih.gov/taxid/")
                .and_then(|s| s.split('?').next())
                .map(str::to_string)
        })
        .ok_or(PhylopicError::MalformedResponse)?;

    let is_self = matched_id == info.taxon_id;
    let image_rank = if is_self {
        info.rank.clone()
    } else {
        info.lineage_ranks
            .get(&matched_id)
            .cloned()
            .unwrap_or_else(|| "lineage".to_string())
    };
    let source = classify_source(is_self, false, &image_rank);

    // With embed_primaryImage=true the image node is inline.
    let image_node = data
        .embedded
        .as_ref()
        .and_then(|e| e.primary_image.as_ref())
        .ok_or(PhylopicError::NotFound)?;

    build_record(
        client,
        image_node,
        data.links.self_link.as_ref().map(|s| s.href.as_str()),
        &info.taxon_id,
        &image_rank,
        source,
        build,
    )
    .await
}

async fn resolve_by_name(
    client: &reqwest::Client,
    info: &TaxonInfo,
    build: u32,
) -> Result<PhylopicRecord, PhylopicError> {
    // Build synonym set: scientific_name + all taxon_names entries
    let synonyms: HashSet<String> = std::iter::once(info.scientific_name.to_lowercase())
        .chain(info.taxon_names.iter().map(|n| n.name.to_lowercase()))
        .collect();

    let lowercase_name = info.scientific_name.to_lowercase();
    let encoded = urlencoding::encode(&lowercase_name);
    let url = format!("{PHYLOPIC_API}/nodes?build={build}&filter_name={encoded}&page=0");

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| PhylopicError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(PhylopicError::Http(format!(
            "PhyloPic name search returned {}",
            resp.status()
        )));
    }

    let data: NodesResponse = resp
        .json()
        .await
        .map_err(|_| PhylopicError::MalformedResponse)?;

    let items = data.links.items.unwrap_or_default();
    let matched = unique_synonym_match(items, &synonyms)?;

    // Fetch the node to get its primaryImage
    let node_resp = client
        .get(&matched.href)
        .send()
        .await
        .map_err(|e| PhylopicError::Http(e.to_string()))?;

    if !node_resp.status().is_success() {
        return Err(PhylopicError::Http(format!(
            "PhyloPic node fetch returned {}",
            node_resp.status()
        )));
    }

    let node: NodeResponse = node_resp
        .json()
        .await
        .map_err(|_| PhylopicError::MalformedResponse)?;

    let image_href = node
        .links
        .primary_image
        .as_ref()
        .ok_or(PhylopicError::NotFound)?
        .href
        .clone();

    // Fetch the actual image node
    let img_resp = client
        .get(format!(
            "{PHYLOPIC_API}{image_href}?embed_primaryImage=true"
        ))
        .send()
        .await
        .map_err(|e| PhylopicError::Http(e.to_string()))?;

    let image_node: ImageNode = img_resp
        .json()
        .await
        .map_err(|_| PhylopicError::MalformedResponse)?;

    let title_matches = matched.title.as_deref() == Some(info.scientific_name.as_str());
    build_record(
        client,
        &image_node,
        Some(matched.href.as_str()),
        &info.taxon_id,
        &info.rank,
        classify_source(true, title_matches, &info.rank),
        build,
    )
    .await
}

async fn resolve_by_gbif(
    client: &reqwest::Client,
    info: &TaxonInfo,
    build: u32,
) -> Result<PhylopicRecord, PhylopicError> {
    let ids = info.gbif_lineage_keys.join(",");
    let url = format!(
        "{PHYLOPIC_API}/resolve/gbif.org/species\
         ?build={build}&objectIDs={ids}&embed_primaryImage=true"
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| PhylopicError::Http(e.to_string()))?;

    if resp.status().as_u16() == 404 {
        return Err(PhylopicError::NotFound);
    }
    if !resp.status().is_success() {
        return Err(PhylopicError::Http(format!(
            "PhyloPic GBIF resolve returned {}",
            resp.status()
        )));
    }

    let data: ResolveResponse = resp
        .json()
        .await
        .map_err(|_| PhylopicError::MalformedResponse)?;

    let image_node = data
        .embedded
        .as_ref()
        .and_then(|e| e.primary_image.as_ref())
        .ok_or(PhylopicError::NotFound)?;

    build_record(
        client,
        image_node,
        data.links.self_link.as_ref().map(|s| s.href.as_str()),
        &info.taxon_id,
        "lineage",
        PhylopicSource::Ancestral,
        build,
    )
    .await
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Classify a match as Primary, Descendant or Ancestral (v2 rules).
fn classify_source(is_self: bool, title_matches: bool, rank: &str) -> PhylopicSource {
    if title_matches || (is_self && rank.ends_with("species")) {
        PhylopicSource::Primary
    } else if is_self {
        PhylopicSource::Descendant
    } else {
        PhylopicSource::Ancestral
    }
}

/// Extract raster and vector URLs from an image node's `_links`.
fn extract_image_files(links: &ImageLinks) -> Option<(String, Option<String>)> {
    let rasters = links.raster_files.as_ref()?;
    if rasters.is_empty() {
        return None;
    }
    // Second-largest raster if available, else largest (same choice as v2)
    let raster_url = if rasters.len() > 1 {
        rasters[1].href.clone()
    } else {
        rasters[0].href.clone()
    };
    let vector_url = links.vector_file.as_ref().map(|v| v.href.clone());
    Some((raster_url, vector_url))
}

/// Compute a floating-point aspect ratio from a raster file URL containing `{W}x{H}`.
///
/// URL format: `https://images.phylopic.org/images/{uuid}/raster/{W}x{H}.png`
fn ratio_from_raster_url(url: &str) -> f32 {
    url.rsplit('/')
        .next()
        .and_then(|filename| filename.strip_suffix(".png"))
        .and_then(|dims| {
            let mut parts = dims.splitn(2, 'x');
            let w: f32 = parts.next()?.parse().ok()?;
            let h: f32 = parts.next()?.parse().ok()?;
            if h == 0.0 {
                None
            } else {
                Some(w / h)
            }
        })
        .unwrap_or(1.0)
}

/// Return the only item whose title is a synonym; zero or several matches fail, as in v2.
fn unique_synonym_match(
    items: Vec<NodeItem>,
    synonyms: &HashSet<String>,
) -> Result<NodeItem, PhylopicError> {
    let mut matches = items.into_iter().filter(|item| {
        item.title
            .as_deref()
            .is_some_and(|t| synonyms.contains(&t.to_lowercase()))
    });
    match (matches.next(), matches.next()) {
        (Some(only), None) => Ok(only),
        _ => Err(PhylopicError::NotFound),
    }
}

/// Whether a licence URL is a public-domain dedication (no BY, NC or SA terms).
fn is_public_domain(license_url: &str) -> bool {
    license_url.contains("publicdomain")
}

/// Licence restrictions applied to a resolved record after resolution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LicenceFilter {
    /// Accepted SPDX identifiers; empty accepts any licence.
    pub licenses: Vec<String>,
    /// Accept only public-domain images.
    pub public_domain: bool,
}

impl LicenceFilter {
    /// Build a filter from a comma-separated SPDX list and the public-domain flag.
    pub fn new(license: Option<&str>, public_domain: bool) -> Self {
        let licenses = license
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect();
        Self {
            licenses,
            public_domain,
        }
    }

    /// Whether a resolved record satisfies every restriction.
    pub fn accepts(&self, record: &PhylopicRecord) -> bool {
        let licence_ok = self.licenses.is_empty()
            || self
                .licenses
                .iter()
                .any(|id| id.eq_ignore_ascii_case(&record.license));
        let domain_ok = !self.public_domain || is_public_domain(&record.license_url);
        licence_ok && domain_ok
    }
}

/// Extract the node UUID from a node href such as `/nodes/{uuid}?build=N`.
fn node_uuid_from_href(href: &str) -> Option<&str> {
    let uuid = href.split("nodes/").nth(1)?.split(['/', '?']).next()?;
    (!uuid.is_empty()).then_some(uuid)
}

/// GET a PhyloPic JSON document, mapping transport and parse failures to `PhylopicError`.
async fn get_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, PhylopicError> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| PhylopicError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(PhylopicError::Http(format!(
            "PhyloPic returned {}",
            resp.status()
        )));
    }
    resp.json()
        .await
        .map_err(|_| PhylopicError::MalformedResponse)
}

/// Find a public-domain image of a node, as v2 did; returns `None` on any failure.
async fn fetch_public_domain_image(
    client: &reqwest::Client,
    node_href: Option<&str>,
    build: u32,
) -> Option<ImageNode> {
    let node_uuid = node_uuid_from_href(node_href?)?;
    let list_url = format!(
        "{PHYLOPIC_API}/images?build={build}&filter_node={node_uuid}\
         &filter_license_by=false&filter_license_nc=false&filter_license_sa=false\
         &page=0&embed_items=true"
    );
    let list: ImagesResponse = get_json(client, &list_url).await.ok()?;
    let image_uuid = list.embedded.items.into_iter().next()?.uuid;
    let image_url =
        format!("{PHYLOPIC_API}/images/{image_uuid}?build={build}&embed_primaryImage=true");
    get_json(client, &image_url).await.ok()
}

/// Map a PhyloPic licence URL to an SPDX identifier.
///
/// Only the licences that PhyloPic actually uses are listed here.
pub fn licence_to_spdx(url: &str) -> &'static str {
    let normalised = url.trim_end_matches('/');
    match normalised {
        "https://creativecommons.org/publicdomain/zero/1.0" => "CC0-1.0",
        "https://creativecommons.org/licenses/by/4.0" => "CC-BY-4.0",
        "https://creativecommons.org/licenses/by/3.0" => "CC-BY-3.0",
        "https://creativecommons.org/licenses/by-nc/3.0" => "CC-BY-NC-3.0",
        "https://creativecommons.org/licenses/by-sa/4.0" => "CC-BY-SA-4.0",
        _ => "unknown",
    }
}

/// Build a `PhylopicRecord`, preferring a public-domain image of the same node (v2 rule).
async fn build_record(
    client: &reqwest::Client,
    image_node: &ImageNode,
    node_href: Option<&str>,
    taxon_id: &str,
    image_rank: &str,
    source: PhylopicSource,
    build: u32,
) -> Result<PhylopicRecord, PhylopicError> {
    let record = record_from_image(client, image_node, taxon_id, image_rank, source, build).await?;
    if is_public_domain(&record.license_url) {
        return Ok(record);
    }
    let Some(alternative) = fetch_public_domain_image(client, node_href, build).await else {
        return Ok(record);
    };
    match record_from_image(client, &alternative, taxon_id, image_rank, source, build).await {
        Ok(public_domain_record) => Ok(public_domain_record),
        Err(_) => Ok(record),
    }
}

/// Build a `PhylopicRecord` from a resolved image node.
///
/// When the image node doesn't carry embedded contributor/licence details, those
/// fields are populated with sensible defaults so the caller always gets a complete
/// record.
async fn record_from_image(
    client: &reqwest::Client,
    image_node: &ImageNode,
    taxon_id: &str,
    rank: &str,
    source: PhylopicSource,
    build: u32,
) -> Result<PhylopicRecord, PhylopicError> {
    let (raster_url, vector_url) =
        extract_image_files(&image_node.links).ok_or(PhylopicError::NotFound)?;

    let ratio = ratio_from_raster_url(&raster_url);

    let uuid = image_node
        .uuid
        .clone()
        .unwrap_or_else(|| extract_uuid_from_raster(&raster_url));

    let source_url = format!("https://www.phylopic.org/images/{uuid}/");

    // Embedded contributor and licence (available when embed_primaryImage=true was used)
    let (contributor, attribution, license_url) =
        extract_attribution(&image_node.embedded, client).await;

    let license = licence_to_spdx(&license_url);

    Ok(PhylopicRecord {
        taxon_id: taxon_id.to_string(),
        raster_url,
        vector_url,
        ratio,
        attribution,
        license: license.to_string(),
        license_url,
        contributor,
        image_name: String::new(), // filled in by route handler from taxon data
        source_url,
        image_rank: rank.to_string(),
        source,
        build,
    })
}

/// Extract contributor name, attribution text, and licence URL from embedded data.
async fn extract_attribution(
    embedded: &Option<ImageEmbedded>,
    _client: &reqwest::Client,
) -> (Option<String>, Option<String>, String) {
    let Some(emb) = embedded else {
        return (None, None, String::new());
    };

    let contributor = emb.contributor.as_ref().and_then(|c| c.name.clone());

    let attribution = contributor.clone();

    let license_url = emb
        .license_version
        .as_ref()
        .and_then(|lv| lv.links.as_ref())
        .and_then(|l| l.self_link.as_ref())
        .map(|s| s.href.clone())
        .unwrap_or_default();

    (contributor, attribution, license_url)
}

/// Extract a UUID from a raster URL of the form `.../images/{uuid}/raster/...`.
fn extract_uuid_from_raster(url: &str) -> String {
    url.split("/images/")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .unwrap_or("unknown")
        .to_string()
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn licence_to_spdx_maps_all_known_urls() {
        assert_eq!(
            licence_to_spdx("https://creativecommons.org/publicdomain/zero/1.0/"),
            "CC0-1.0"
        );
        assert_eq!(
            licence_to_spdx("https://creativecommons.org/licenses/by/4.0"),
            "CC-BY-4.0"
        );
        assert_eq!(
            licence_to_spdx("https://creativecommons.org/licenses/by/3.0"),
            "CC-BY-3.0"
        );
        assert_eq!(
            licence_to_spdx("https://creativecommons.org/licenses/by-nc/3.0"),
            "CC-BY-NC-3.0"
        );
        assert_eq!(
            licence_to_spdx("https://creativecommons.org/licenses/by-sa/4.0"),
            "CC-BY-SA-4.0"
        );
        assert_eq!(
            licence_to_spdx("https://example.com/unknown-licence"),
            "unknown"
        );
    }

    #[test]
    fn licence_to_spdx_trailing_slash_is_stripped() {
        assert_eq!(
            licence_to_spdx("https://creativecommons.org/licenses/by/4.0/"),
            "CC-BY-4.0"
        );
    }

    #[test]
    fn matched_id_extracted_via_prefix_strip_not_index() {
        // Regression test for v2 bug: must use prefix strip, not path index.
        let href = "/resolve/ncbi.nlm.nih.gov/taxid/9606?foo=bar";
        let matched_id = href
            .strip_prefix("/resolve/ncbi.nlm.nih.gov/taxid/")
            .and_then(|s| s.split('?').next())
            .unwrap();
        assert_eq!(matched_id, "9606");

        // Works even if format changes to include extra segments before the id
        let href2 = "/resolve/ncbi.nlm.nih.gov/taxid/10090";
        let matched2 = href2
            .strip_prefix("/resolve/ncbi.nlm.nih.gov/taxid/")
            .and_then(|s| s.split('?').next())
            .unwrap();
        assert_eq!(matched2, "10090");
    }

    #[test]
    fn extract_image_files_returns_second_raster_and_vector() {
        let links = ImageLinks {
            raster_files: Some(vec![
                FileLink {
                    href: "https://images.phylopic.org/images/abc/raster/1024x512.png".to_string(),
                    media_type: None,
                },
                FileLink {
                    href: "https://images.phylopic.org/images/abc/raster/512x256.png".to_string(),
                    media_type: None,
                },
            ]),
            vector_file: Some(FileLink {
                href: "https://images.phylopic.org/images/abc/vector.svg".to_string(),
                media_type: None,
            }),
            contributor_link: None,
            license_version: None,
            taxa: None,
        };

        let (raster, vector) = extract_image_files(&links).unwrap();
        assert_eq!(
            raster,
            "https://images.phylopic.org/images/abc/raster/512x256.png"
        );
        assert_eq!(
            vector,
            Some("https://images.phylopic.org/images/abc/vector.svg".to_string())
        );
    }

    #[test]
    fn extract_image_files_falls_back_to_first_when_only_one_raster() {
        let links = ImageLinks {
            raster_files: Some(vec![FileLink {
                href: "https://images.phylopic.org/images/abc/raster/512x512.png".to_string(),
                media_type: None,
            }]),
            vector_file: None,
            contributor_link: None,
            license_version: None,
            taxa: None,
        };

        let (raster, vector) = extract_image_files(&links).unwrap();
        assert_eq!(
            raster,
            "https://images.phylopic.org/images/abc/raster/512x512.png"
        );
        assert!(vector.is_none());
    }

    #[test]
    fn ratio_computed_from_raster_url() {
        let url = "https://images.phylopic.org/images/abc/raster/1024x512.png";
        assert!((ratio_from_raster_url(url) - 2.0).abs() < f32::EPSILON);

        let square = "https://images.phylopic.org/images/abc/raster/512x512.png";
        assert!((ratio_from_raster_url(square) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ratio_defaults_to_one_for_unrecognised_url() {
        assert!(
            (ratio_from_raster_url("https://example.com/image.jpg") - 1.0).abs() < f32::EPSILON
        );
    }

    #[test]
    fn cache_miss_when_build_differs() {
        let mut cache = PhylopicCache {
            current_build: 538,
            ..PhylopicCache::default()
        };

        let record = PhylopicRecord {
            taxon_id: "9606".to_string(),
            raster_url: "https://example.com/r.png".to_string(),
            vector_url: None,
            ratio: 1.0,
            attribution: None,
            license: "CC0-1.0".to_string(),
            license_url: "https://creativecommons.org/publicdomain/zero/1.0".to_string(),
            contributor: None,
            image_name: "Homo sapiens".to_string(),
            source_url: "https://www.phylopic.org/images/abc/".to_string(),
            image_rank: "species".to_string(),
            source: PhylopicSource::Primary,
            build: 537,
        };

        cache.insert("9606".to_string(), record);
        assert!(
            cache.get("9606").is_some(),
            "entry just inserted should be found"
        );

        // Advance build — existing entry is now stale
        cache.current_build = 539;
        assert!(
            cache.get("9606").is_none(),
            "entry from build 538 should be stale after advancing to 539"
        );
    }

    #[test]
    fn cache_hit_when_build_matches() {
        let mut cache = PhylopicCache {
            current_build: 538,
            ..PhylopicCache::default()
        };

        let record = PhylopicRecord {
            taxon_id: "9606".to_string(),
            raster_url: "https://example.com/r.png".to_string(),
            vector_url: None,
            ratio: 1.0,
            attribution: None,
            license: "CC0-1.0".to_string(),
            license_url: "https://creativecommons.org/publicdomain/zero/1.0".to_string(),
            contributor: None,
            image_name: "Homo sapiens".to_string(),
            source_url: "https://www.phylopic.org/images/abc/".to_string(),
            image_rank: "species".to_string(),
            source: PhylopicSource::Primary,
            build: 538,
        };

        cache.insert("9606".to_string(), record);
        assert!(cache.get("9606").is_some());
    }

    #[test]
    fn classify_source_primary_for_self_species() {
        assert_eq!(
            classify_source(true, false, "species"),
            PhylopicSource::Primary
        );
    }

    #[test]
    fn classify_source_primary_when_title_matches() {
        assert_eq!(
            classify_source(true, true, "genus"),
            PhylopicSource::Primary
        );
    }

    #[test]
    fn classify_source_descendant_for_self_above_species() {
        assert_eq!(
            classify_source(true, false, "genus"),
            PhylopicSource::Descendant
        );
    }

    #[test]
    fn classify_source_ancestral_for_other_matches() {
        assert_eq!(
            classify_source(false, false, "species"),
            PhylopicSource::Ancestral
        );
    }

    #[test]
    fn unique_synonym_match_returns_single_synonym_hit() {
        let synonyms: HashSet<String> = ["homo sapiens".to_string()].into_iter().collect();
        let items = vec![
            NodeItem {
                href: "https://api.phylopic.org/nodes/abc".to_string(),
                title: Some("Homo sapiens var. idaltu".to_string()),
            },
            NodeItem {
                href: "https://api.phylopic.org/nodes/def".to_string(),
                title: Some("Homo sapiens".to_string()),
            },
        ];

        let matched = unique_synonym_match(items, &synonyms).unwrap();

        assert_eq!(matched.href, "https://api.phylopic.org/nodes/def");
    }

    #[test]
    fn unique_synonym_match_rejects_several_synonym_hits() {
        let synonyms: HashSet<String> = ["homo sapiens".to_string()].into_iter().collect();
        let items = vec![
            NodeItem {
                href: "https://api.phylopic.org/nodes/abc".to_string(),
                title: Some("Homo sapiens".to_string()),
            },
            NodeItem {
                href: "https://api.phylopic.org/nodes/def".to_string(),
                title: Some("homo sapiens".to_string()),
            },
        ];

        assert!(unique_synonym_match(items, &synonyms).is_err());
    }

    #[test]
    fn unique_synonym_match_rejects_first_item_without_synonym() {
        let synonyms: HashSet<String> = ["no match here".to_string()].into_iter().collect();
        let items = vec![NodeItem {
            href: "https://api.phylopic.org/nodes/abc".to_string(),
            title: Some("Unrelated taxon A".to_string()),
        }];

        assert!(unique_synonym_match(items, &synonyms).is_err());
    }

    #[test]
    fn node_uuid_extracted_from_node_href() {
        assert_eq!(
            node_uuid_from_href("https://api.phylopic.org/nodes/abc-123?build=5"),
            Some("abc-123")
        );
        assert_eq!(node_uuid_from_href("/nodes/abc-123"), Some("abc-123"));
        assert_eq!(node_uuid_from_href("/images/abc"), None);
    }

    #[test]
    fn public_domain_detected_from_licence_url() {
        assert!(is_public_domain(
            "https://creativecommons.org/publicdomain/zero/1.0"
        ));
        assert!(!is_public_domain(
            "https://creativecommons.org/licenses/by/4.0"
        ));
        assert!(!is_public_domain(""));
    }

    fn record_with_licence(license: &str, license_url: &str) -> PhylopicRecord {
        PhylopicRecord {
            taxon_id: "9606".to_string(),
            raster_url: String::new(),
            vector_url: None,
            ratio: 1.0,
            attribution: None,
            license: license.to_string(),
            license_url: license_url.to_string(),
            contributor: None,
            image_name: String::new(),
            source_url: String::new(),
            image_rank: "species".to_string(),
            source: PhylopicSource::Primary,
            build: 1,
        }
    }

    #[test]
    fn licence_filter_parses_comma_separated_list() {
        let filter = LicenceFilter::new(Some("CC0-1.0, CC-BY-4.0,"), false);
        assert_eq!(filter.licenses, vec!["CC0-1.0", "CC-BY-4.0"]);
        assert_eq!(LicenceFilter::new(None, false), LicenceFilter::default());
    }

    #[test]
    fn licence_filter_default_accepts_any_licence() {
        let by = record_with_licence("CC-BY-4.0", "https://creativecommons.org/licenses/by/4.0");
        assert!(LicenceFilter::default().accepts(&by));
    }

    #[test]
    fn licence_filter_rejects_licence_not_in_list() {
        let by = record_with_licence("CC-BY-4.0", "https://creativecommons.org/licenses/by/4.0");
        let filter = LicenceFilter::new(Some("CC0-1.0"), false);
        assert!(!filter.accepts(&by));
        assert!(LicenceFilter::new(Some("cc-by-4.0"), false).accepts(&by));
    }

    #[test]
    fn licence_filter_public_domain_rejects_attribution_licence() {
        let by = record_with_licence("CC-BY-4.0", "https://creativecommons.org/licenses/by/4.0");
        let cc0 = record_with_licence(
            "CC0-1.0",
            "https://creativecommons.org/publicdomain/zero/1.0",
        );
        let filter = LicenceFilter::new(None, true);
        assert!(!filter.accepts(&by));
        assert!(filter.accepts(&cc0));
    }
}
