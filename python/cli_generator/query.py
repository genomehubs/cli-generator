"""Site-agnostic query builder for genomehubs search APIs.

``QueryBuilder`` accumulates query state across multiple calls — matching the
multi-stage MCP pipeline — and serialises to YAML for use with
:func:`~cli_generator.build_url`.

Typical usage::

    from cli_generator import QueryBuilder, build_url

    q = QueryBuilder("taxon")
    q.set_taxa(["Mammalia"], filter_type="tree")
    q.set_rank("species")
    q.add_attribute("genome_size", operator="lt", value="3G", modifiers=["min", "direct"])
    q.add_field("genome_size", modifiers=["min"])
    q.set_names(["scientific_name"])

    url = build_url(
        q.to_query_yaml(),
        q.to_params_yaml(),
        api_base="https://goat.genomehubs.org/api",
        api_version="v2",
        endpoint="search",
    )
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    import pandas
    import polars

# ── Public re-export ──────────────────────────────────────────────────────────

__all__ = ["QueryBuilder", "ReportBuilder", "probe_api_capability"]


def probe_api_capability(api_base: str) -> str:
    """Probe an API base URL and return its capability level.

    Calls ``{api_base}/v3/status``. If the response includes ``/search`` in
    the ``supported`` list, returns ``"v3"``. Falls back to ``"v2"`` on any
    error or missing endpoint.

    Args:
        api_base: Base URL of the API, e.g. ``"https://goat.genomehubs.org/api"``.

    Returns:
        ``"v3"`` or ``"v2"``.
    """
    import json
    import urllib.request

    try:
        with urllib.request.urlopen(f"{api_base}/v3/status", timeout=5) as resp:
            data = json.loads(resp.read().decode())
        if "/search" in data.get("supported", []):
            return "v3"
    except Exception:
        pass
    return "v2"


class QueryBuilder:
    """Accumulate a genomehubs ``SearchQuery`` incrementally.

    Each ``set_*`` / ``add_*`` method returns ``self`` to support method
    chaining as well as the staged MCP-tool calling pattern.

    Args:
        index: Index to search — one of ``"taxon"``, ``"assembly"``,
            ``"sample"``.  More indexes may be available depending on the
            site.
    """

    def __init__(
        self,
        index: str,
    ) -> None:
        self._index = index
        self._taxa: list[str] = []
        self._assemblies: list[str] = []
        self._samples: list[str] = []
        self._rank: str | None = None
        self._taxon_filter_type: str = "name"
        self._attributes: list[dict[str, Any]] = []
        self._fields: list[dict[str, Any]] = []
        self._names: list[str] = []
        self._ranks: list[str] = []
        # Exclusion filters (field-level)
        self._exclude_ancestral: list[str] = []
        self._exclude_descendant: list[str] = []
        self._exclude_direct: list[str] = []
        self._exclude_missing: list[str] = []
        # Lineage rank summary aggregation specs
        self._lineage_rank_summary: list[dict[str, Any]] = []
        # QueryParams
        self._size: int = 10
        self._page: int = 1
        self._sort_by: str | None = None
        self._sort_order: str = "asc"
        self._include_estimates: bool = True
        self._tidy: bool = False
        self._taxonomy: str = "ncbi"
        # ID-set filter: restrict results to exactly this set of IDs
        self._id_set: list[str] | None = None
        # ID type: which field to filter on (taxon, assembly, sample, feature)
        self._id_type: str | None = None
        # Pre-parsed YAML overrides (set by from_v2_url; take priority in to_*_yaml)
        self._query_yaml_override: str | None = None
        self._params_yaml_override: str | None = None
        # Named sub-queries for chain substitution (queryA= style).
        self._named_queries: dict[str, dict[str, Any]] | None = None

    # ── Identifiers ──────────────────────────────────────────────────────────

    def chain_query(
        self,
        query_key: str,
        query_string: str,
        *,
        index: str | None = None,
        limit: int | None = None,
        inherit_scope: bool | None = None,
    ) -> "QueryBuilder":
        """Register a named sub-query for chain substitution.

        Values in attribute filters may reference this query using dot-notation:
        ``add_attribute("taxon_id", "eq", "queryA.taxon_id")``.

        Args:
            query_key: Name for this sub-query, e.g. ``"queryA"``.
            query_string: Filter expression, e.g.
                ``"assembly_span>1000000000"`` or
                ``"assembly--assembly_span>1000000000"`` (v2 cross-index format).
            index: Target index for the sub-query.  ``None`` inherits the
                parent query's index.
            limit: Maximum results to fetch (default 500, max 10 000).
            inherit_scope: Whether to scope the sub-query inside the parent
                taxon tree.  ``None`` uses the default (same-index → inherit).
        """
        spec: dict[str, Any] = {"filter_expr": query_string}
        if index is not None:
            spec["index"] = index
        if limit is not None:
            spec["limit"] = limit
        if inherit_scope is not None:
            spec["inherit_scope"] = inherit_scope
        if self._named_queries is None:
            self._named_queries = {}
        self._named_queries[query_key] = spec
        return self

    def set_taxa(
        self,
        taxa: list[str],
        filter_type: str = "name",
    ) -> "QueryBuilder":
        """Set the taxon filter.

        Args:
            taxa: Taxon names or IDs.  Prefix with ``!`` for exclusion,
                e.g. ``["Mammalia", "!Felis"]``.
            filter_type: One of ``"name"``, ``"tree"``, ``"lineage"``.
        """
        self._taxa = list(taxa)
        self._taxon_filter_type = filter_type
        return self

    def set_rank(self, rank: str) -> "QueryBuilder":
        """Restrict results to a single taxonomic rank, e.g. ``"species"``."""
        self._rank = rank
        return self

    def set_assemblies(self, accessions: list[str]) -> "QueryBuilder":
        """Filter by assembly accession IDs."""
        self._assemblies = list(accessions)
        return self

    def set_samples(self, accessions: list[str]) -> "QueryBuilder":
        """Filter by sample accession IDs."""
        self._samples = list(accessions)
        return self

    # ── Attributes ───────────────────────────────────────────────────────────

    def add_attribute(
        self,
        name: str,
        operator: str | None = None,
        value: str | list[str] | None = None,
        modifiers: list[str] | None = None,
    ) -> "QueryBuilder":
        """Add an attribute filter.

        Args:
            name: Field name or synonym, e.g. ``"genome_size"``.
            operator: Comparison operator — one of ``"eq"``, ``"ne"``,
                ``"lt"``, ``"le"``, ``"gt"``, ``"ge"``, ``"exists"``,
                ``"missing"``.
            value: Scalar string or list of strings.  Size suffixes
                ``"G"``/``"M"``/``"K"`` are accepted for byte fields.
            modifiers: Summary modifiers (``"min"``, ``"max"``, …) and/or
                status modifiers (``"direct"``, ``"ancestral"``,
                ``"descendant"``, ``"estimated"``, ``"missing"``).
        """
        entry: dict[str, Any] = {"name": name}
        if operator is not None:
            entry["operator"] = operator
        if value is not None:
            entry["value"] = value
        if modifiers:
            entry["modifier"] = list(modifiers)
        self._attributes.append(entry)
        return self

    def add_field(
        self,
        name: str,
        modifiers: list[str] | None = None,
    ) -> "QueryBuilder":
        """Request a field in the response.

        Accepts either the plain field name or the ``"field:modifier"``
        shorthand.  For example, ``add_field("assembly_span:min")`` is
        equivalent to ``add_field("assembly_span", modifiers=["min"])``.

        Args:
            name: Field name, e.g. ``"genome_size"``, or shorthand with a
                modifier suffix, e.g. ``"assembly_span:min"``.
            modifiers: Additional summary modifiers, e.g. ``["min", "max"]``.
        """
        bare_name = name
        colon_modifiers: list[str] = []
        if ":" in name:
            bare_name, colon_mod = name.split(":", 1)
            colon_modifiers = [colon_mod]
        entry: dict[str, Any] = {"name": bare_name}
        all_modifiers = colon_modifiers + list(modifiers or [])
        if all_modifiers:
            entry["modifier"] = all_modifiers
        self._fields.append(entry)
        return self

    def field_modifiers(self) -> list[str]:
        """Return the ``__modifier`` column names implied by any field requests with modifiers.

        Summary modifiers (``min``, ``max``, …) and status modifiers (``direct``,
        ``ancestral``, ``descendant``) all produce a ``{field}__modifier`` column in
        the parsed output when explicitly requested via ``:modifier`` syntax.
        This is distinct from the automatically-added ``{field}__source`` metadata
        column, which is never in this list.

        Pass the return value as ``keep_columns_json`` to
        :func:`values_only` or :func:`annotated_values` so that these explicitly
        requested columns survive the ``__*`` stripping step.
        """
        result: list[str] = []
        for field in self._fields:
            field_name = field["name"] if isinstance(field, dict) else str(field)
            mods: list[str] = field.get("modifier", []) if isinstance(field, dict) else []
            result.extend(f"{field_name}__{mod}" for mod in mods)
        return result

    def to_flat_records(
        self,
        lineage_summary: dict[str, dict[str, str | list[str]]] | None = None,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> list[dict[str, Any]]:
        """Fetch results and return flat records, optionally with lineage summary columns.

        Calls :meth:`search`, parses the response, and attaches per-ancestor
        aggregation columns when ``lineage_summary`` is provided.

        ``lineage_summary`` controls which distributions to attach and how to
        reduce them.  Its structure is ``{rank: {field: mode_or_modes}}``.
        Modes:

        - ``"top"`` — most common keyword value
        - ``"top_n:<N>"`` — top-N values as a list
        - ``"all"`` — full distribution dict
        - ``"count"`` — distinct value count
        - ``"min"`` / ``"max"`` / ``"avg"`` — individual stats
        - ``"stats"`` — all four stats as ``{rank}_{field}__min`` etc.

        The ``lineage_rank_summary`` specs must have been set on the builder
        (via :meth:`set_lineage_rank_summary`) so that the API computes the
        aggregations.

        Args:
            lineage_summary: Reduction config, or ``None`` to return plain flat
                records without lineage columns.
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            List of flat record dicts.  Each dict contains the standard
            identity and attribute columns.  When ``lineage_summary`` is
            supplied, extra columns such as ``genus__assembly_level`` or
            ``genus__genome_size__min`` are appended.

        Example::

            records = (
                QueryBuilder("taxon")
                .set_taxa(["Canidae"])
                .set_rank("species")
                .set_fields(["genome_size", "assembly_level"])
                .set_lineage_rank_summary([
                    {"rank": "genus", "fields": ["assembly_level", "genome_size"]},
                ])
                .to_flat_records(
                    lineage_summary={
                        "genus": {
                            "assembly_level": "top",
                            "genome_size": "stats",
                        }
                    }
                )
            )
        """
        import json

        from cli_generator import parse_search_json as _parse_search_json
        from cli_generator import parse_search_with_lineage_summary as _parse_search_with_lineage_summary

        if lineage_summary is not None:
            # Build lineage_rank_summary specs from the config keys so the API
            # returns the aggregation block and auto-includes lineage in results.
            specs = [{"rank": rank, "fields": list(fields.keys())} for rank, fields in lineage_summary.items()]
            saved_lrs = self._lineage_rank_summary
            self._lineage_rank_summary = specs
            try:
                raw = self.search(format="json", api_base=api_base, api_version=api_version)
            finally:
                self._lineage_rank_summary = saved_lrs
            data_str = json.dumps(raw) if isinstance(raw, dict) else raw
            return list(json.loads(_parse_search_with_lineage_summary(data_str, json.dumps(lineage_summary))))

        raw = self.search(format="json", api_base=api_base, api_version=api_version)
        data_str = json.dumps(raw) if isinstance(raw, dict) else raw
        return list(json.loads(_parse_search_json(data_str)))

    def to_tidy_records(
        self,
        records: list[dict[str, Any]] | str | None = None,
        lineage_summary: dict[str, dict[str, str | list[str]]] | None = None,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> list[dict[str, Any]]:
        """Reshape flat records from ``parse_search_json`` into long/tidy format.

        Each flat record is exploded so that every bare field becomes its own
        row with columns ``"field"``, ``"value"``, ``"source"``, and any identity
        columns (``taxon_id``, ``scientific_name``, ``taxon_rank``, …) present in
        the source record.

        When ``lineage_summary`` is provided and ``records`` is ``None``, the
        full search response is parsed with
        :func:`~cli_generator.parse_search_with_lineage_summary` so that
        lineage summary columns appear as additional tidy rows.  Column naming
        follows the same convention as :meth:`to_flat_records`.

        Explicitly-requested modifier columns (from ``field:modifier`` requests,
        e.g. ``assembly_span__min``) are emitted as separate rows with ``"field"``
        set to ``"{bare}:{modifier}"`` and ``"source"`` as ``None``.

        This is the natural input for ``pandas.melt`` or R's ``tidyr::pivot_longer``.

        Args:
            records: Flat record dicts, a JSON string of flat records, or ``None``
                to automatically call :meth:`search` and parse the response.
            lineage_summary: Reduction config for lineage summary columns (same
                format as :meth:`to_flat_records`).  Only used when ``records``
                is ``None``.
            api_base: Base URL of the API (used only when ``records`` is ``None``).
            api_version: API version string (used only when ``records`` is ``None``).

        Returns:
            List of dicts in tidy (long) format.
        """
        import json

        from cli_generator import parse_search_json as _parse_search_json
        from cli_generator import parse_search_with_lineage_summary as _parse_search_with_lineage_summary
        from cli_generator import to_tidy_records as _to_tidy_records

        if records is None:
            raw = self.search(format="json", api_base=api_base, api_version=api_version)
            data_str = json.dumps(raw) if isinstance(raw, dict) else raw
            if lineage_summary is not None:
                records_json = _parse_search_with_lineage_summary(data_str, json.dumps(lineage_summary))
            else:
                records_json = _parse_search_json(data_str)
        elif isinstance(records, str):
            records_json = records
        else:
            records_json = json.dumps(records)
        return list(json.loads(_to_tidy_records(records_json)))

    def set_attributes(
        self,
        attributes: list[dict[str, Any]],
    ) -> "QueryBuilder":
        """Replace all attribute filters at once.

        Convenience method for setting multiple filters in a single call.
        Each entry must be a dict with at least a ``"name"`` key; ``"operator"``,
        ``"value"``, and ``"modifier"`` are optional.

        Args:
            attributes: List of attribute dicts, e.g.
                ``[{"name": "genome_size", "operator": "ge", "value": "1G"}]``.
        """
        self._attributes = [dict(a) for a in attributes]
        return self

    def set_fields(
        self,
        fields: list[str | dict[str, Any]],
    ) -> "QueryBuilder":
        """Replace the field selection at once.

        Convenience method for setting multiple fields in a single call.
        Each entry may be a plain field name string or a dict with ``"name"``
        and optional ``"modifier"`` keys.

        Args:
            fields: List of field names or field dicts, e.g.
                ``["genome_size", {"name": "assembly_span", "modifier": ["min"]}]``.
        """
        self._fields = [{"name": f} if isinstance(f, str) else dict(f) for f in fields]
        return self

    def set_names(self, name_classes: list[str]) -> "QueryBuilder":
        """Set the name classes to include, e.g. ``["scientific_name"]``."""
        self._names = list(name_classes)
        return self

    def set_ranks(self, ranks: list[str]) -> "QueryBuilder":
        """Set the lineage rank columns to include, e.g. ``["genus", "family"]``."""
        self._ranks = list(ranks)
        return self

    def set_lineage_rank_summary(
        self,
        specs: list[dict[str, Any]],
    ) -> "QueryBuilder":
        """Set the per-rank ancestor aggregation specs for a lineage rank summary.

        Each spec must have a ``"rank"`` key and a ``"fields"`` key listing the
        attribute names to aggregate across species in each ancestor.  The API
        validates that all field names exist.

        Args:
            specs: List of rank specs, e.g.
                ``[{"rank": "genus", "fields": ["assembly_level", "genome_size"]}]``.
        """
        self._lineage_rank_summary = [dict(s) for s in specs]
        return self

    # ── Exclusion filters (field-level) ──────────────────────────────────────

    @staticmethod
    def _normalise_fields(fields: list[str] | None) -> list[str]:
        """Return a shallow copy of ``fields``, or an empty list when ``None``."""
        return list(fields) if fields is not None else []

    def set_exclude_ancestral(self, fields: list[str] | None) -> "QueryBuilder":
        """Exclude records with ancestrally derived estimated values for specified fields.

        Args:
            fields: Field names to exclude ancestral estimates for. Pass ``None``
                or an empty list to clear.
        """
        self._exclude_ancestral = self._normalise_fields(fields)
        return self

    def add_exclude_ancestral(self, field: str) -> "QueryBuilder":
        """Add a field to exclude ancestrally derived values for."""
        if field not in self._exclude_ancestral:
            self._exclude_ancestral.append(field)
        return self

    def set_exclude_descendant(self, fields: list[str] | None) -> "QueryBuilder":
        """Exclude records with descendant-derived estimated values for specified fields."""
        self._exclude_descendant = self._normalise_fields(fields)
        return self

    def add_exclude_descendant(self, field: str) -> "QueryBuilder":
        """Add a field to exclude descendant-derived values for."""
        if field not in self._exclude_descendant:
            self._exclude_descendant.append(field)
        return self

    def set_exclude_direct(self, fields: list[str] | None) -> "QueryBuilder":
        """Exclude records with directly estimated values for specified fields."""
        self._exclude_direct = self._normalise_fields(fields)
        return self

    def add_exclude_direct(self, field: str) -> "QueryBuilder":
        """Add a field to exclude direct estimates for."""
        if field not in self._exclude_direct:
            self._exclude_direct.append(field)
        return self

    def set_exclude_missing(self, fields: list[str] | None) -> "QueryBuilder":
        """Exclude records with missing values for specified fields."""
        self._exclude_missing = self._normalise_fields(fields)
        return self

    def add_exclude_missing(self, field: str) -> "QueryBuilder":
        """Add a field to exclude records with missing values for."""
        if field not in self._exclude_missing:
            self._exclude_missing.append(field)
        return self

    def set_exclude_derived(self, fields: list[str] | None) -> "QueryBuilder":
        """Exclude all non-direct estimates (excludes ancestral and descendant).

        Shorthand for: exclude ancestral + exclude descendant.
        Keeps only directly estimated values.

        Args:
            fields: Field names to restrict to direct estimates only.
                Pass ``None`` or an empty list to clear this exclusion.
        """
        normalised = self._normalise_fields(fields)
        self._exclude_ancestral = normalised
        self._exclude_descendant = list(normalised)
        return self

    def set_exclude_estimated(self, fields: list[str] | None) -> "QueryBuilder":
        """Exclude ancestral estimates and missing values.

        Shorthand for: exclude ancestral + exclude missing.
        Keeps directly estimated values and descendant-derived estimates.

        Args:
            fields: Field names to restrict to confirmed (non-ancestral) values.
                Pass ``None`` or an empty list to clear this exclusion.
        """
        normalised = self._normalise_fields(fields)
        self._exclude_ancestral = normalised
        self._exclude_missing = list(normalised)
        return self

    # ── Query params ─────────────────────────────────────────────────────────

    def set_size(self, size: int) -> "QueryBuilder":
        """Set the page size (number of results per page)."""
        self._size = size
        return self

    def set_page(self, page: int) -> "QueryBuilder":
        """Set the page number (1-based)."""
        self._page = page
        return self

    def set_sort(self, field: str, order: str = "asc") -> "QueryBuilder":
        """Sort results by ``field`` in ``order`` (``"asc"`` or ``"desc"``)."""
        self._sort_by = field
        self._sort_order = order
        return self

    def set_include_estimates(self, value: bool) -> "QueryBuilder":
        """Control whether estimated values are included (default ``True``)."""
        self._include_estimates = value
        return self

    def set_taxonomy(self, taxonomy: str) -> "QueryBuilder":
        """Set the taxonomy source, e.g. ``"ncbi"`` or ``"ott"``."""
        self._taxonomy = taxonomy
        return self

    def set_id_set(self, taxon_ids: list[str | int]) -> "QueryBuilder":
        """Restrict results to exactly the supplied taxon IDs.

        Injected as an ES ``terms`` filter ANDed with the main query.
        Maximum 65,536 IDs (ES hard limit). A structured error is returned
        for larger sets.

        Args:
            taxon_ids: List of integer taxon IDs to include.

        Example::

            qb.set_id_set([10090, 10116, 9606])

        Returns:
            Self for chaining.
        """
        self._id_set = [str(x) for x in taxon_ids]
        return self

    def set_id_type(self, id_type: str) -> "QueryBuilder":
        """Specify which ID field to filter on when using ``set_id_set``.

        When combined with ``set_id_set``, determines the ES field to filter on:
        - ``"taxon"`` → ``taxon_id``
        - ``"assembly"`` → ``assembly_id``
        - ``"sample"`` → ``sample_id``
        - ``"feature"`` → ``feature_id``

        If not specified, defaults to the current index type.

        Args:
            id_type: One of ``"taxon"``, ``"assembly"``, ``"sample"``,
                ``"feature"``.

        Returns:
            Self for chaining.
        """
        self._id_type = id_type
        return self

    # ── Serialisation ────────────────────────────────────────────────────────

    def to_query_yaml(self) -> str:
        """Serialise the query into a YAML string for :func:`build_url`."""
        if self._query_yaml_override is not None:
            return self._query_yaml_override

        import yaml  # type: ignore[import-untyped]

        doc: dict[str, Any] = {"index": self._index}

        if self._taxa:
            doc["taxa"] = self._taxa
        if self._assemblies:
            doc["assemblies"] = self._assemblies
        if self._samples:
            doc["samples"] = self._samples
        if self._rank:
            doc["rank"] = self._rank
        if self._taxon_filter_type != "name":
            doc["taxon_filter_type"] = self._taxon_filter_type
        if self._attributes:
            doc["attributes"] = self._attributes
        if self._fields:
            doc["fields"] = self._fields
        if self._names:
            doc["names"] = self._names
        if self._ranks:
            doc["ranks"] = self._ranks
        if self._exclude_ancestral:
            doc["exclude_ancestral"] = self._exclude_ancestral
        if self._exclude_descendant:
            doc["exclude_descendant"] = self._exclude_descendant
        if self._exclude_direct:
            doc["exclude_direct"] = self._exclude_direct
        if self._exclude_missing:
            doc["exclude_missing"] = self._exclude_missing
        if self._lineage_rank_summary:
            doc["lineage_rank_summary"] = self._lineage_rank_summary
        if self._named_queries:
            doc["named_queries"] = self._named_queries

        return yaml.safe_dump(doc, sort_keys=False)

    def to_params_yaml(self) -> str:
        """Serialise the execution parameters into a YAML string."""
        if self._params_yaml_override is not None:
            return self._params_yaml_override

        import yaml  # type: ignore[import-untyped]

        doc: dict[str, Any] = {
            "size": self._size,
            "page": self._page,
            "include_estimates": self._include_estimates,
            "tidy": self._tidy,
            "taxonomy": self._taxonomy,
        }
        if self._sort_by:
            doc["sort_by"] = self._sort_by
            doc["sort_order"] = self._sort_order
        if self._id_set:
            doc["id_set"] = self._id_set
        if self._id_type:
            doc["id_type"] = self._id_type

        return yaml.safe_dump(doc, sort_keys=False)

    # ── URL + API calls ───────────────────────────────────────────────────────

    def _post_json(self, url: str, payload: dict[str, Any]) -> Any:
        """POST a JSON payload and return the parsed response body.

        Args:
            url: Full URL to POST to.
            payload: Dict to serialise as JSON request body.

        Returns:
            Parsed JSON response as a Python object.
        """
        import json
        import urllib.request

        req = urllib.request.Request(
            url,
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req) as resp:
            return json.loads(resp.read().decode("utf-8"))

    @classmethod
    def from_v2_url(cls, url: str) -> "QueryBuilder | ReportBuilder":
        """Reconstruct a builder from a v2 API or UI URL.

        Detects whether the URL is a search or report URL and returns the
        appropriate builder type.  Report URLs (path ends in ``/report`` or
        the query string contains ``report=``) return a :class:`ReportBuilder`
        with an embedded query so that :meth:`ReportBuilder.run` can be called
        without supplying a separate :class:`QueryBuilder`.

        Args:
            url: A full v2 API or UI URL, e.g.
                ``"https://goat.genomehubs.org/api/v2/search?tax_name=Primates&fields=genome_size"``
                or
                ``"https://goat.genomehubs.org/report?report=histogram&x=genome_size&result=taxon"``.

        Returns:
            A populated :class:`QueryBuilder` for search URLs, or a
            :class:`ReportBuilder` (with an embedded query) for report URLs.

        Raises:
            ValueError: If URL parsing or YAML serialisation fails.
        """
        import urllib.parse

        from . import query_yaml_from_url_params as _parse_search
        from . import report_yaml_from_url_params as _parse_report

        parsed = urllib.parse.urlparse(url)
        qs = urllib.parse.parse_qs(parsed.query)
        is_report = "report" in qs or parsed.path.rstrip("/").endswith("/report")

        if is_report:
            query_yaml, params_yaml, report_yaml = _parse_report(url)
            # Build the embedded QueryBuilder
            qb: QueryBuilder = cls.__new__(cls)
            qb._index = "taxon"
            qb._taxa = []
            qb._assemblies = []
            qb._samples = []
            qb._rank = None
            qb._taxon_filter_type = "name"
            qb._attributes = []
            qb._fields = []
            qb._names = []
            qb._ranks = []
            qb._exclude_ancestral = []
            qb._exclude_descendant = []
            qb._exclude_direct = []
            qb._exclude_missing = []
            qb._size = 10
            qb._page = 1
            qb._sort_by = None
            qb._sort_order = "asc"
            qb._include_estimates = True
            qb._tidy = False
            qb._taxonomy = "ncbi"
            qb._query_yaml_override = query_yaml
            qb._params_yaml_override = params_yaml
            rb = ReportBuilder.__new__(ReportBuilder)
            rb._doc = {}
            rb._report_yaml_override = report_yaml
            rb._embedded_query_builder = qb
            return rb

        query_yaml, params_yaml = _parse_search(url)
        qb = cls.__new__(cls)
        qb._index = "taxon"
        qb._taxa = []
        qb._assemblies = []
        qb._samples = []
        qb._rank = None
        qb._taxon_filter_type = "name"
        qb._attributes = []
        qb._fields = []
        qb._names = []
        qb._ranks = []
        qb._exclude_ancestral = []
        qb._exclude_descendant = []
        qb._exclude_direct = []
        qb._exclude_missing = []
        qb._size = 10
        qb._page = 1
        qb._sort_by = None
        qb._sort_order = "asc"
        qb._include_estimates = True
        qb._tidy = False
        qb._taxonomy = "ncbi"
        qb._query_yaml_override = query_yaml
        qb._params_yaml_override = params_yaml
        return qb

    def to_v2_url(
        self,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v2",
        endpoint: str = "search",
    ) -> str:
        """Build the full v2 API URL for this query without making a network call.

        Args:
            api_base: Base URL of the API.
            api_version: API version string.
            endpoint: API endpoint, e.g. ``"search"`` or ``"count"``.

        Returns:
            Fully encoded URL string.
        """
        from . import build_url as _build_url

        return _build_url(
            self.to_query_yaml(),
            self.to_params_yaml(),
            api_base,
            api_version,
            endpoint,
        )

    def to_url(
        self,
        api_base: str = "https://goat.genomehubs.org/api",
        endpoint: str = "search",
    ) -> str:
        """Build a v3 GET API URL for this query without making a network call.

        Returns a URL for the ``GET /api/v3/{endpoint}`` endpoint, which
        accepts a GoaT UI URL in the ``?url=`` parameter.

        If the query uses features that are not recoverable from a URL
        (currently: output name classes set via :meth:`set_names` or rank
        columns set via :meth:`set_ranks`), a :class:`RuntimeWarning` is
        emitted and the returned URL will silently omit those features.

        Args:
            api_base: Base URL of the API without trailing slash.
            endpoint: API endpoint, e.g. ``"search"`` or ``"count"``.

        Returns:
            Fully encoded v3 GET URL string.
        """
        import warnings
        from urllib.parse import quote

        from . import build_ui_url as _build_ui_url

        incomplete: list[str] = []
        if self._names:
            incomplete.append(f"name classes ({self._names!r})")
        if self._ranks:
            incomplete.append(f"rank columns ({self._ranks!r})")
        if incomplete:
            warnings.warn(
                f"to_url() cannot fully represent this query: "
                f"{', '.join(incomplete)} will be omitted from the URL. "
                f"Use to_v2_url() or the POST endpoint for full fidelity.",
                RuntimeWarning,
                stacklevel=2,
            )

        ui_url = _build_ui_url(
            self.to_query_yaml(),
            self.to_params_yaml(),
            "https://goat.genomehubs.org",
            endpoint,
        )
        encoded = quote(ui_url, safe="")
        return f"{api_base}/v3/{endpoint}?url={encoded}"

    def to_ui_url(
        self,
        ui_base: str = "https://goat.genomehubs.org",
        endpoint: str = "search",
    ) -> str:
        """Build the full UI URL for this query without making a network call.

        The UI URL targets the web interface rather than the REST API — no API
        version component is inserted.

        Args:
            ui_base: Base URL of the web UI without trailing slash.
            endpoint: API endpoint, e.g. ``"search"``.

        Returns:
            Fully encoded URL string.
        """
        from . import build_ui_url as _build_ui_url

        return _build_ui_url(
            self.to_query_yaml(),
            self.to_params_yaml(),
            ui_base,
            endpoint,
        )

    def count(
        self,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> int:
        """Fetch the count of records matching this query.

        Uses the v3 POST ``/count`` endpoint by default.  Pass
        ``api_version="v2"`` to fall back to the legacy GET ``/search``
        path (returns ``status.hits`` with ``size=0``).

        Args:
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Number of matching records.
        """
        import json

        from . import parse_response_status

        if api_version != "v3":
            import urllib.request

            counter = QueryBuilder(self._index)
            counter.merge(self)
            counter.set_size(0)
            url = counter.to_v2_url(api_base, api_version, "search")
            with urllib.request.urlopen(url) as resp:
                body_text = resp.read().decode("utf-8")
            status = json.loads(parse_response_status(body_text))
            return int(status.get("hits") or 0)

        data = self._post_json(
            f"{api_base}/{api_version}/count",
            {"query_yaml": self.to_query_yaml(), "params_yaml": self.to_params_yaml()},
        )
        status = json.loads(parse_response_status(json.dumps(data)))
        return int(status.get("hits") or 0)

    def search(
        self,
        format: str = "json",
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Fetch results for this query.

        Uses the v3 POST ``/search`` endpoint by default.  Pass
        ``api_version="v2"`` to fall back to the legacy GET path.

        Args:
            format: Response format — ``"json"`` (default) or ``"tsv"``.
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Parsed JSON (dict) for ``format="json"``; raw text for ``"tsv"``.
        """
        import json
        import urllib.request

        if api_version != "v3":
            url = self.to_v2_url(api_base, api_version, "search")
            accept = {
                "json": "application/json",
                "tsv": "text/tab-separated-values",
            }.get(format, "application/json")
            req = urllib.request.Request(url, headers={"Accept": accept})
            with urllib.request.urlopen(req) as resp:
                raw = resp.read().decode()
            return json.loads(raw) if format == "json" else raw

        data = self._post_json(
            f"{api_base}/{api_version}/search",
            {"query_yaml": self.to_query_yaml(), "params_yaml": self.to_params_yaml()},
        )
        return data

    def search_all(
        self,
        max_records: int | None = None,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> list[dict[str, Any]]:
        """Fetch all matching records using cursor-based pagination.

        With v3 (default) uses repeated POST ``/search`` calls with
        ``search_after`` cursors.  With ``api_version="v2"`` falls back to the
        legacy GET ``/searchPaginated`` path.

        Args:
            max_records: Maximum total records to return.  ``None`` means no
                limit.
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            List of flat record dicts in the same format as
            :func:`~cli_generator.parse_search_json` output.
        """
        import json

        from . import parse_paginated_json, parse_search_json

        CHUNK_SIZE = 1_000
        cap: float = float("inf") if max_records is None else float(max_records)
        all_records: list[dict[str, Any]] = []

        if api_version != "v3":
            import urllib.parse
            import urllib.request

            search_after: list[Any] | None = None
            while True:
                base_url = self.to_v2_url(api_base, api_version, "searchPaginated")
                sep = "&" if "?" in base_url else "?"
                url = base_url + f"{sep}size={CHUNK_SIZE}"
                if search_after is not None:
                    url += "&searchAfter=" + urllib.parse.quote(json.dumps(search_after))
                req = urllib.request.Request(url, headers={"Accept": "application/json"})
                with urllib.request.urlopen(req) as resp:
                    raw: str = resp.read().decode()
                page: dict[str, Any] = json.loads(parse_paginated_json(raw))
                records: list[dict[str, Any]] = page.get("records", [])
                remaining = int(cap) - len(all_records)
                all_records.extend(records[:remaining])
                if not page.get("hasMore", False) or len(all_records) >= cap:
                    break
                search_after = page.get("searchAfter")
            return all_records

        # v3: cursor-based POST loop
        search_after_v3: Any = None
        orig_size = self._size
        self.set_size(CHUNK_SIZE)
        try:
            while True:
                payload: dict[str, Any] = {
                    "query_yaml": self.to_query_yaml(),
                    "params_yaml": self.to_params_yaml(),
                }
                if search_after_v3 is not None:
                    payload["search_after"] = search_after_v3
                resp_data = self._post_json(f"{api_base}/{api_version}/search", payload)
                records = json.loads(parse_search_json(json.dumps(resp_data)))
                remaining = int(cap) - len(all_records)
                all_records.extend(records[:remaining])
                search_after_v3 = resp_data.get("search_after")
                total = resp_data.get("status", {}).get("hits", 0)
                if not search_after_v3 or len(all_records) >= min(int(cap), total):
                    break
        finally:
            self._size = orig_size

        return all_records[:max_records]

    def search_df(
        self,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> "pandas.DataFrame":
        """Execute a search and return results as a pandas DataFrame.

        Requires: ``pip install pandas``

        Args:
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            pandas DataFrame with results.

        Raises:
            ImportError: If pandas is not installed.
        """
        import io

        try:
            import pandas as pd  # type: ignore[import-untyped]
        except ModuleNotFoundError as e:
            raise ImportError("search_df() requires pandas. Install it with:\n\n" "    pip install pandas\n") from e

        tsv = self.search(format="tsv", api_base=api_base, api_version=api_version)
        return pd.read_csv(io.StringIO(tsv), sep="\t")

    def search_polars(
        self,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> "polars.DataFrame":
        """Execute a search and return results as a polars DataFrame.

        Requires: ``pip install polars``

        Args:
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            polars DataFrame with results.

        Raises:
            ImportError: If polars is not installed.
        """
        import io

        try:
            import polars as pl  # type: ignore[import-untyped]
        except ModuleNotFoundError as e:
            raise ImportError("search_polars() requires polars. Install it with:\n\n" "    pip install polars\n") from e

        tsv = self.search(format="tsv", api_base=api_base, api_version=api_version)
        return pl.read_csv(io.StringIO(tsv), separator="\t")

    def report(
        self,
        report: "ReportBuilder",
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Run a report query against the v3 ``/report`` endpoint.

        Args:
            report: A :class:`ReportBuilder` instance describing the report
                configuration.
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Raw ``report`` dict from the response.
        """
        data = self._post_json(
            f"{api_base}/{api_version}/report",
            {
                "query_yaml": self.to_query_yaml(),
                "params_yaml": self.to_params_yaml(),
                "report_yaml": report.to_report_yaml(),
                **({"display": report._display} if report._display is not None else {}),
                **({"include_plot_spec": True} if report._include_plot_spec else {}),
            },
        )
        if data.get("plot_spec"):
            return data
        return data.get("report", data)

    def search_batch(
        self,
        queries: list["QueryBuilder"],
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Execute multiple searches in a single batch request.

        Combines multiple QueryBuilder objects into a single batch API call,
        returning document hits for each query.

        Args:
            queries: List of QueryBuilder objects to search in batch.
            api_base: Base URL of the API.
            api_version: API version string (default: "v3").

        Returns:
            List of parsed result objects, one per input query.

        Raises:
            ValueError: If more than 100 queries are provided.
        """
        import json
        import urllib.request

        from . import parse_batch_json

        if len(queries) > 100:
            raise ValueError("maximum 100 searches per batch request")

        url = f"{api_base}/{api_version}/search/batch"
        payload = {
            "searches": [
                {
                    "query_yaml": q.to_query_yaml(),
                    "params_yaml": q.to_params_yaml(),
                }
                for q in queries
            ]
        }
        req = urllib.request.Request(
            url,
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req) as resp:
            body_text = resp.read().decode("utf-8")

        parsed_results = []
        data = json.loads(parse_batch_json(body_text))
        for result in data.get("results", []):
            parsed_results.append(result)
        return parsed_results

    def count_batch(
        self,
        queries: list["QueryBuilder"],
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> list[int]:
        """Get hit counts for multiple queries in a single batch request.

        Combines multiple QueryBuilder objects into a single batch count API call,
        returning the hit count for each query.

        Args:
            queries: List of QueryBuilder objects to count in batch.
            api_base: Base URL of the API.
            api_version: API version string (default: "v3").

        Returns:
            List of hit counts, one per input query.

        Raises:
            ValueError: If more than 100 queries are provided.
        """
        import json
        import urllib.request

        from . import parse_batch_json

        if len(queries) > 100:
            raise ValueError("maximum 100 searches per batch request")

        url = f"{api_base}/{api_version}/count/batch"
        payload = {
            "searches": [
                {
                    "query_yaml": q.to_query_yaml(),
                    "params_yaml": q.to_params_yaml(),
                }
                for q in queries
            ]
        }
        req = urllib.request.Request(
            url,
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req) as resp:
            body_text = resp.read().decode("utf-8")

        data = json.loads(parse_batch_json(body_text))
        counts = []
        for result in data.get("results", []):
            counts.append(int(result.get("status", {}).get("hits") or 0))
        return counts

    def record(
        self,
        record_id: str,
        result: str | None = None,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Fetch a single record by ID or identifier.

        Args:
            record_id: Record ID to fetch (required).
            result: Result type (``"taxon"``, ``"assembly"``, ``"sample"``); defaults to the
                builder's current index.
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Parsed record object with all available fields.
        """
        if not record_id:
            raise ValueError("record() requires a non-empty record_id")
        import json
        import urllib.parse
        import urllib.request

        from . import parse_record_json

        result_type = result or self._index or "taxon"
        params = urllib.parse.urlencode({"recordId": record_id, "result": result_type})
        url = f"{api_base}/{api_version}/record?{params}"
        with urllib.request.urlopen(url) as resp:
            body_text = resp.read().decode("utf-8")
        return json.loads(parse_record_json(body_text))

    def record_batch(
        self,
        record_ids: "list[str]",
        result: str | None = None,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Fetch up to 1,000 records by ID in a single request.

        Uses the ``POST /record/batch`` endpoint which issues a single ES ``_mget``
        call, making it far more efficient than repeated ``record()`` calls.

        Args:
            record_ids: List of record IDs to fetch (max 1,000; required).
            result: Result type (``"taxon"``, ``"assembly"``, ``"sample"``); defaults to the
                builder's current index.
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Parsed batch record response with a ``records`` list.

        Example::

            ids = ["taxon-9606", "taxon-10090"]
            resp = QueryBuilder("taxon").record_batch(ids)
            for item in resp["records"]:
                print(item["recordId"], item["record"])
        """
        if not record_ids:
            raise ValueError("record_batch() requires a non-empty record_ids list")
        import json
        import urllib.request

        result_type = result or self._index or "taxon"
        payload = json.dumps({"record_ids": record_ids, "result": result_type}).encode("utf-8")
        url = f"{api_base}/{api_version}/record/batch"
        req = urllib.request.Request(
            url,
            data=payload,
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req) as resp:
            body_text = resp.read().decode("utf-8")
        return json.loads(body_text)

    def positional(
        self,
        report: str,
        group_by: str,
        assemblies: list[str],
        *,
        feature_type: str | None = None,
        window_size: int | None = None,
        reorient: bool = True,
        max_features: int = 10_000,
        cat: str | None = None,
        cat_opts: str | None = None,
        filter: list[dict[str, Any]] | None = None,
        regions: dict[str, Any] | None = None,
        max_connections_per_group: int | None = None,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Run a positional report (oxford / ribbon / painting / circos) via ``POST /positional``.

        The feature index only supports ``taxon_id`` and ``ancestors`` for taxon
        filtering.  Taxon names in the current query are automatically resolved to
        taxon IDs via a lookup against the taxon index.

        Args:
            report: Sub-type — one of ``"oxford"``, ``"ribbon"``, ``"painting"``, or ``"circos"``.
            group_by: Attribute key used as shared marker identifier (e.g. ``"busco_gene"``).
            assemblies: Assembly IDs to compare.  Oxford requires exactly 2; painting
                requires exactly 1; ribbon/circos require ≥ 2.
            feature_type: Optional ``primary_type`` filter (e.g. ``"busco"``).
            window_size: Regional binning in base-pairs.  ``None`` returns individual positions.
            reorient: Auto-orient comparison sequences (default ``True``).
            max_features: Hard cap on features fetched (default 10 000).
            cat: Optional category field for colour (e.g. ``"busco_status"``).
            cat_opts: Category axis options in the standard axis DSL.  List category
                values explicitly, e.g. ``"complete,fragmented,missing;;5"``.
            filter: List of attribute filter dicts.  Each dict must have ``field``,
                ``operator``, ``value``, and ``target`` keys.  See the API docs for
                the full schema.
            regions: Region computation config dict.  Keys: ``cat``, ``name_to_cat``,
                ``bounds`` (``"feature_ends"`` or ``"midpoints"``), ``min_features``,
                ``max_expansion``.
            max_connections_per_group: Hard cap on connections per group for M:N
                feature mappings.  ``None`` uses the server default (25).
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Raw ``report`` dict from the response.
        """
        import yaml as _yaml

        positional_doc: dict[str, Any] = {
            "report": report,
            "group_by": group_by,
            "assemblies": list(assemblies),
        }
        if feature_type is not None:
            positional_doc["feature_type"] = feature_type
        if window_size is not None:
            positional_doc["window_size"] = window_size
        if not reorient:
            positional_doc["reorient"] = False
        if max_features != 10_000:
            positional_doc["max_features"] = max_features
        if cat is not None:
            positional_doc["cat"] = cat
        if cat_opts is not None:
            positional_doc["cat_opts"] = cat_opts
        if filter:
            positional_doc["filter"] = filter
        if regions is not None:
            positional_doc["regions"] = regions
        if max_connections_per_group is not None:
            positional_doc["max_connections_per_group"] = max_connections_per_group

        data = self._post_json(
            f"{api_base}/{api_version}/positional",
            {
                "query_yaml": self.to_query_yaml(),
                "positional_yaml": _yaml.dump(positional_doc, default_flow_style=False),
            },
        )
        return data.get("report", data)

    def oxford(
        self,
        group_by: str,
        assemblies: list[str],
        *,
        feature_type: str | None = None,
        window_size: int | None = None,
        reorient: bool = True,
        max_features: int = 10_000,
        cat: str | None = None,
        cat_opts: str | None = None,
        filter: list[dict[str, Any]] | None = None,
        regions: dict[str, Any] | None = None,
        max_connections_per_group: int | None = None,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Run an Oxford dot-plot positional report (exactly 2 assemblies).

        Convenience wrapper around :meth:`positional` with ``report="oxford"``.

        Args:
            group_by: Attribute key used as shared marker identifier (e.g. ``"busco_gene"``).
            assemblies: Exactly 2 assembly IDs to compare.
            feature_type: Optional ``primary_type`` filter.
            window_size: Regional binning in base-pairs.
            reorient: Auto-orient comparison sequences (default ``True``).
            max_features: Hard cap on features fetched (default 10 000).
            cat: Optional category field for colour.
            cat_opts: Category axis options in the standard axis DSL.
            filter: Attribute filter list (see :meth:`positional`).
            regions: Region config dict (see :meth:`positional`).
            max_connections_per_group: M:N connection cap (see :meth:`positional`).
            api_base: Base URL of the API.
            api_version: API version string.

        Returns:
            Raw ``report`` dict from the response.
        """
        return self.positional(
            "oxford",
            group_by,
            assemblies,
            feature_type=feature_type,
            window_size=window_size,
            reorient=reorient,
            max_features=max_features,
            cat=cat,
            cat_opts=cat_opts,
            filter=filter,
            regions=regions,
            max_connections_per_group=max_connections_per_group,
            api_base=api_base,
            api_version=api_version,
        )

    def ribbon(
        self,
        group_by: str,
        assemblies: list[str],
        *,
        feature_type: str | None = None,
        window_size: int | None = None,
        reorient: bool = True,
        max_features: int = 10_000,
        cat: str | None = None,
        cat_opts: str | None = None,
        filter: list[dict[str, Any]] | None = None,
        regions: dict[str, Any] | None = None,
        max_connections_per_group: int | None = None,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Run a ribbon / synteny positional report (N ≥ 2 assemblies).

        Convenience wrapper around :meth:`positional` with ``report="ribbon"``.

        Args:
            group_by: Attribute key used as shared marker identifier.
            assemblies: At least 2 assembly IDs.  Assembly 0 is the reference.
            feature_type: Optional ``primary_type`` filter.
            window_size: Regional binning in base-pairs.
            reorient: Auto-orient comparison sequences (default ``True``).
            max_features: Hard cap on features fetched (default 10 000).
            cat: Optional category field for colour.
            cat_opts: Category axis options in the standard axis DSL.
            filter: Attribute filter list (see :meth:`positional`).
            regions: Region config dict (see :meth:`positional`).
            max_connections_per_group: M:N connection cap (see :meth:`positional`).
            api_base: Base URL of the API.
            api_version: API version string.

        Returns:
            Raw ``report`` dict from the response.
        """
        return self.positional(
            "ribbon",
            group_by,
            assemblies,
            feature_type=feature_type,
            window_size=window_size,
            reorient=reorient,
            max_features=max_features,
            cat=cat,
            cat_opts=cat_opts,
            filter=filter,
            regions=regions,
            max_connections_per_group=max_connections_per_group,
            api_base=api_base,
            api_version=api_version,
        )

    def painting(
        self,
        group_by: str,
        assembly: str,
        *,
        feature_type: str | None = None,
        window_size: int | None = None,
        max_features: int = 10_000,
        cat: str | None = None,
        cat_opts: str | None = None,
        filter: list[dict[str, Any]] | None = None,
        regions: dict[str, Any] | None = None,
        max_connections_per_group: int | None = None,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Run a chromosome painting positional report (single assembly).

        Convenience wrapper around :meth:`positional` with ``report="painting"``.

        Args:
            group_by: Attribute key used as shared marker identifier.
            assembly: A single assembly ID.
            feature_type: Optional ``primary_type`` filter.
            window_size: Regional binning in base-pairs.
            max_features: Hard cap on features fetched (default 10 000).
            cat: Optional category field for colour (e.g. ``"busco_status"``).
            cat_opts: Category axis options in the standard axis DSL.
            filter: Attribute filter list (see :meth:`positional`).
            regions: Region config dict (see :meth:`positional`).
            max_connections_per_group: M:N connection cap (see :meth:`positional`).
            api_base: Base URL of the API.
            api_version: API version string.

        Returns:
            Raw ``report`` dict from the response.
        """
        return self.positional(
            "painting",
            group_by,
            [assembly],
            feature_type=feature_type,
            window_size=window_size,
            max_features=max_features,
            cat=cat,
            cat_opts=cat_opts,
            filter=filter,
            regions=regions,
            max_connections_per_group=max_connections_per_group,
            api_base=api_base,
            api_version=api_version,
        )

    def hybrid_positional(
        self,
        report: str,
        group_by: str,
        local_files: "list[dict[str, Any]]",
        *,
        remote_assemblies: "list[str] | None" = None,
        reorient: bool = True,
        cat: "str | None" = None,
        window_size: "int | None" = None,
        max_connections_per_group: int = 0,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Run a hybrid positional report combining remote and local assembly data.

        Parses one or more local BUSCO / feature files and, when
        ``remote_assemblies`` is supplied, fetches remote features via
        ``POST /api/v3/positional`` and joins them with the local data.
        When no remote assemblies are given, the plot is computed entirely
        from the local files (no API call).

        Each entry in ``local_files`` is a dict with keys:

        - ``"busco"`` — full text of a BUSCO ``full_table.tsv`` file (required).
        - ``"assembly_id"`` — label for the assembly (required).
        - ``"fai"`` — full text of a ``.fai`` file (optional).
        - ``"lengths"`` — full text of a two-column lengths TSV (optional).

        If neither ``"fai"`` nor ``"lengths"`` is supplied, sequence lengths are
        derived from ``max(feature.end)`` per sequence and
        ``"lengthsDerived": true`` is set in the output assembly metadata.

        Args:
            report:                    Sub-type — one of ``"oxford"``, ``"ribbon"``,
                                       or ``"painting"``.
            group_by:                  Shared marker identifier attribute
                                       (e.g. ``"busco_gene"``).
            local_files:               List of local assembly dicts (see above).
            remote_assemblies:         Optional list of API assembly IDs to fetch
                                       remotely and use as the reference.
            reorient:                  Auto-orient comparison sequences (default ``True``).
            cat:                       Category field for colour coding.
            window_size:               Regional binning in bp (``None`` for individual
                                       positions).
            max_connections_per_group: Cap on M:N connections (``0`` → default 25).
            api_base:                  Base URL of the API.
            api_version:               API version string (default: ``"v3"``).

        Returns:
            Report dict in the same format as :meth:`positional`.
        """
        import json

        from . import hybrid_positional as _hybrid_positional
        from . import (
            parse_busco_tsv,
            parse_fai,
            parse_lengths_tsv,
        )
        from . import positional_from_features as _positional_from_features

        # Parse each local file entry into a LocalFeatureSet dict
        local_sets: list[dict[str, Any]] = []
        for entry in local_files:
            asm_id = entry["assembly_id"]
            busco_text = entry["busco"]
            raw = json.loads(parse_busco_tsv(asm_id, busco_text))
            if "error" in raw:
                raise ValueError(f"parse_busco_tsv failed for assembly '{asm_id}': {raw['error']}")

            # Populate sequence_lengths from .fai or lengths TSV if supplied
            if "fai" in entry:
                lengths_map = json.loads(parse_fai(entry["fai"]))
                if "error" in lengths_map:
                    raise ValueError(f"parse_fai failed for assembly '{asm_id}': {lengths_map['error']}")
                raw["sequence_lengths"] = lengths_map
                raw["lengths_derived"] = False
            elif "lengths" in entry:
                lengths_map = json.loads(parse_lengths_tsv(entry["lengths"]))
                if "error" in lengths_map:
                    raise ValueError(f"parse_lengths_tsv failed for assembly '{asm_id}': {lengths_map['error']}")
                raw["sequence_lengths"] = lengths_map
                raw["lengths_derived"] = False

            local_sets.append(raw)

        ws = window_size or 0

        # All-local mode: no remote assemblies
        if not remote_assemblies:
            result_json = _positional_from_features(
                json.dumps(local_sets),
                report,
                reorient,
                cat or "",
                ws,
                max_connections_per_group,
                "",
            )
            result = json.loads(result_json)
            if "error" in result:
                raise RuntimeError(f"positional_from_features failed: {result['error']}")
            return result

        # Hybrid mode: fetch remote reference, then join with local
        import yaml as _yaml

        positional_doc: dict[str, Any] = {
            "report": report,
            "group_by": group_by,
            "assemblies": list(remote_assemblies),
        }
        if cat is not None:
            positional_doc["cat"] = cat
        if window_size is not None:
            positional_doc["window_size"] = window_size
        if not reorient:
            positional_doc["reorient"] = False
        if max_connections_per_group:
            positional_doc["max_connections_per_group"] = max_connections_per_group

        remote_data = self._post_json(
            f"{api_base}/{api_version}/positional",
            {
                "query_yaml": self.to_query_yaml(),
                "positional_yaml": _yaml.dump(positional_doc, default_flow_style=False),
            },
        )
        remote_report = remote_data.get("report", remote_data)

        result_json = _hybrid_positional(
            json.dumps(remote_report),
            json.dumps(local_sets),
            reorient,
            max_connections_per_group,
        )
        result = json.loads(result_json)
        if "error" in result:
            raise RuntimeError(f"hybrid_positional failed: {result['error']}")
        return result

    def lookup(
        self,
        search_term: str,
        result: str | None = None,
        size: int = 10,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Lookup records by alternative identifiers (autocomplete/search-as-you-type).

        Args:
            search_term: Search term for lookup (required).
            result: Result type (``"taxon"``, ``"assembly"``, ``"sample"``); defaults to the
                builder's current index.
            size: Number of results to return (default: ``10``).
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Parsed lookup result object.
        """
        if not search_term:
            raise ValueError("lookup() requires a non-empty search_term")
        import json
        import urllib.parse
        import urllib.request

        from . import parse_lookup_json

        result_type = result or self._index or "taxon"
        params = urllib.parse.urlencode(
            {
                "searchTerm": search_term,
                "result": result_type,
                "size": str(size),
            }
        )
        url = f"{api_base}/{api_version}/lookup?{params}"
        with urllib.request.urlopen(url) as resp:
            body_text = resp.read().decode("utf-8")
        return json.loads(parse_lookup_json(body_text))

    def lookup_batch(
        self,
        lookups: "list[str | dict[str, Any]]",
        result: str | None = None,
        size: int = 10,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Resolve multiple search terms to record IDs in a single request.

        Each element of ``lookups`` is either a plain search-term string or a
        dict with keys ``search_term`` (required), ``result`` (optional), and
        ``size`` (optional).  Per-item values override the method-level
        ``result`` and ``size`` defaults.

        Args:
            lookups: List of search terms (strings or dicts).
            result: Default result type for items that don't specify one;
                defaults to the builder's current index (``"taxon"``).
            size: Default page size for items that don't specify one (default 10).
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Parsed batch lookup response object with a ``results`` list parallel
            to the input.

        Example::

            names = ["Homo sapiens", "Mus musculus"]
            resp = QueryBuilder("taxon").lookup_batch(names)
            for item in resp["results"]:
                for hit in item["results"]:
                    print(hit["id"], hit["name"])
        """
        if not lookups:
            raise ValueError("lookup_batch() requires a non-empty lookups list")
        import json
        import urllib.request

        default_result = result or self._index or "taxon"

        def normalise(item: "str | dict[str, Any]") -> "dict[str, Any]":
            if isinstance(item, str):
                return {"search_term": item, "result": default_result, "size": size}
            return {
                "search_term": item["search_term"],
                "result": item.get("result", default_result),
                "size": item.get("size", size),
            }

        payload = json.dumps({"lookups": [normalise(x) for x in lookups]}).encode("utf-8")
        url = f"{api_base}/{api_version}/lookup/batch"
        req = urllib.request.Request(
            url,
            data=payload,
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req) as resp:
            body_text = resp.read().decode("utf-8")
        return json.loads(body_text)

    def phylopic(
        self,
        taxon_id: str,
        taxonomy: str = "ncbi",
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Fetch a PhyloPic silhouette record for a single taxon.

        Queries the ``/phylopic`` proxy endpoint, which resolves the best
        available silhouette from PhyloPic for the given NCBI taxon ID and
        returns it with URL, attribution, and licence metadata.

        Args:
            taxon_id: NCBI taxon ID (required).
            taxonomy: Taxonomy name (default: ``"ncbi"``).
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Silhouette record dict with ``uuid``, ``raster_url``, ``vector_url``,
            ``ratio``, ``attribution``, ``license``, ``license_url``,
            ``source_url``, and ``source`` fields, or ``None`` when no
            silhouette is found.
        """
        if not taxon_id:
            raise ValueError("phylopic() requires a non-empty taxon_id")
        import json
        import urllib.parse
        import urllib.request

        from . import parse_phylopic_json

        params = urllib.parse.urlencode({"taxon_id": taxon_id, "taxonomy": taxonomy})
        url = f"{api_base}/{api_version}/phylopic?{params}"
        with urllib.request.urlopen(url) as resp:
            body_text = resp.read().decode("utf-8")
        return json.loads(parse_phylopic_json(body_text))

    def phylopic_batch(
        self,
        taxon_ids: list[str],
        taxonomy: str = "ncbi",
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> list[Any]:
        """Fetch PhyloPic silhouette records for multiple taxa in one request.

        POSTs up to 200 NCBI taxon IDs to the ``/phylopic/batch`` proxy
        endpoint.  Results are returned as a flat list; taxa with no
        silhouette in PhyloPic are omitted.

        Args:
            taxon_ids: List of NCBI taxon IDs (1–200, required).
            taxonomy: Taxonomy name (default: ``"ncbi"``).
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            List of silhouette record dicts, each including a ``taxon_id`` key
            plus the same fields as :meth:`phylopic`.
        """
        if not taxon_ids:
            raise ValueError("phylopic_batch() requires at least one taxon_id")
        if len(taxon_ids) > 200:
            raise ValueError("phylopic_batch() accepts at most 200 taxon IDs per request")
        import json
        import urllib.request

        from . import parse_phylopic_batch_json

        url = f"{api_base}/{api_version}/phylopic/batch"
        payload = json.dumps({"taxon_ids": taxon_ids, "taxonomy": taxonomy}).encode("utf-8")
        req = urllib.request.Request(
            url,
            data=payload,
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req) as resp:
            body_text = resp.read().decode("utf-8")
        return json.loads(parse_phylopic_batch_json(body_text))

    def metadata(
        self,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> dict:
        """Fetch aggregated metadata in a single request.

        Returns indices, taxonomies, ranks, and versions without requiring
        separate calls to each sub-endpoint.

        Args:
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Dict with ``indices``, ``taxonomies``, ``ranks``, and ``versions`` keys.
        """
        import json
        import urllib.request

        url = f"{api_base}/{api_version}/metadata"
        with urllib.request.urlopen(url) as resp:
            data = json.loads(resp.read().decode("utf-8"))
        return {k: data[k] for k in ("indices", "taxonomies", "ranks", "versions") if k in data}

    def indices(
        self,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> list[str]:
        """Return the list of available index names.

        Args:
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            List of index name strings, e.g. ``["taxon", "assembly", "sample"]``.
        """
        import json
        import urllib.request

        url = f"{api_base}/{api_version}/metadata/indices"
        with urllib.request.urlopen(url) as resp:
            return json.loads(resp.read().decode("utf-8")).get("indices", [])

    def fields(
        self,
        index: str,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> dict:
        """Return field metadata for the given index.

        Args:
            index: Index name, e.g. ``"taxon"`` or ``"assembly"`` (required).
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Dict mapping field name to field metadata dict.
        """
        import json
        import urllib.parse
        import urllib.request

        if not index:
            raise ValueError("fields() requires a non-empty index name")
        params = urllib.parse.urlencode({"result": index})
        url = f"{api_base}/{api_version}/metadata/fields?{params}"
        with urllib.request.urlopen(url) as resp:
            return json.loads(resp.read().decode("utf-8")).get("fields", {})

    def taxonomies(
        self,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> list[str]:
        """Return the list of available taxonomy names.

        Args:
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            List of taxonomy name strings, e.g. ``["ncbi", "ott"]``.
        """
        import json
        import urllib.request

        url = f"{api_base}/{api_version}/metadata/taxonomies"
        with urllib.request.urlopen(url) as resp:
            return json.loads(resp.read().decode("utf-8")).get("taxonomies", [])

    def ranks(
        self,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> list[str]:
        """Return the list of recognised taxonomic rank names.

        Args:
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            List of rank name strings, e.g. ``["species", "genus", ...]``.
        """
        import json
        import urllib.request

        url = f"{api_base}/{api_version}/metadata/ranks"
        with urllib.request.urlopen(url) as resp:
            return json.loads(resp.read().decode("utf-8")).get("ranks", [])

    def summary(
        self,
        record_id: str,
        fields: str,
        result: str | None = None,
        summary: str = "histogram",
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Fetch summary aggregations for specific fields.

        Runs a clade-level ES aggregation (all taxa under ``record_id``) and
        returns either a histogram or a terms distribution for the requested field.

        Args:
            record_id: Taxon ID whose clade is aggregated (required).
            fields: Field name to aggregate (required).
            result: Result type (``"taxon"``, ``"assembly"``, ``"sample"``); defaults to the
                builder's current index.
            summary: Aggregation type — ``"histogram"`` (default) or ``"terms"``.
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Parsed summary object with aggregation results.
        """
        if not record_id:
            raise ValueError("summary() requires a non-empty record_id")
        if not fields:
            raise ValueError("summary() requires a non-empty fields string")
        import json
        import urllib.parse
        import urllib.request

        result_type = result or self._index or "taxon"
        params = urllib.parse.urlencode(
            {
                "recordId": record_id,
                "result": result_type,
                "fields": fields,
                "summary": summary,
            }
        )
        url = f"{api_base}/{api_version}/summary?{params}"
        with urllib.request.urlopen(url) as resp:
            body_text = resp.read().decode("utf-8")
        return json.loads(body_text)

    # ── Utilities ─────────────────────────────────────────────────────────────

    def reset(self) -> "QueryBuilder":
        """Clear all query state while preserving the index and params."""
        self._taxa = []
        self._assemblies = []
        self._samples = []
        self._rank = None
        self._taxon_filter_type = "name"
        self._attributes = []
        self._fields = []
        self._names = []
        self._ranks = []
        self._exclude_ancestral = []
        self._exclude_descendant = []
        self._exclude_direct = []
        self._exclude_missing = []
        return self

    # ── Merging ───────────────────────────────────────────────────────────────

    def merge(self, other: "QueryBuilder") -> "QueryBuilder":
        """Merge non-default state from ``other`` into this builder.

        List fields (taxa, attributes, fields, …) are extended.  Scalar
        fields (rank, filter_type, size, …) are overwritten only when
        ``other`` holds a non-default value, so you can safely merge a
        builder that only touched identifiers with one that only touched
        attributes.

        Args:
            other: Builder whose state will be merged in.  Must use the
                same index as ``self``.

        Returns:
            ``self``, for chaining.

        Raises:
            ValueError: If ``other`` has a different index.
        """
        if other._index != self._index:
            raise ValueError(f"cannot merge builders with different indexes: " f"'{self._index}' vs '{other._index}'")
        # Lists — extend (the two builders typically own disjoint lists)
        self._taxa.extend(other._taxa)
        self._assemblies.extend(other._assemblies)
        self._samples.extend(other._samples)
        self._attributes.extend(other._attributes)
        self._fields.extend(other._fields)
        self._names.extend(other._names)
        self._ranks.extend(other._ranks)
        self._exclude_ancestral.extend(other._exclude_ancestral)
        self._exclude_descendant.extend(other._exclude_descendant)
        self._exclude_direct.extend(other._exclude_direct)
        self._exclude_missing.extend(other._exclude_missing)
        # Scalars — overwrite only if other differs from its default
        if other._rank is not None:
            self._rank = other._rank
        if other._taxon_filter_type != "name":
            self._taxon_filter_type = other._taxon_filter_type
        if other._size != 10:
            self._size = other._size
        if other._page != 1:
            self._page = other._page
        if other._sort_by is not None:
            self._sort_by = other._sort_by
            self._sort_order = other._sort_order
        if not other._include_estimates:
            self._include_estimates = other._include_estimates
        if other._tidy:
            self._tidy = other._tidy
        if other._taxonomy != "ncbi":
            self._taxonomy = other._taxonomy
        return self

    @classmethod
    def combine(cls, *builders: "QueryBuilder") -> "QueryBuilder":
        """Create a new builder by merging all ``builders`` together.

        Useful when parallel MCP tools each produce a partial builder that
        must be combined before building the URL::

            id_builder  = QueryBuilder("taxon").set_taxa(...)
            attr_builder = QueryBuilder("taxon").add_attribute(...)
            q = QueryBuilder.combine(id_builder, attr_builder)
            url = build_url(q.to_query_yaml(), q.to_params_yaml(), ...)

        All builders must share the same index.

        Returns:
            A new :class:`QueryBuilder` containing all merged state.

        Raises:
            ValueError: If no builders are provided, or indexes differ.
        """
        if not builders:
            raise ValueError("combine() requires at least one builder")
        result = cls(builders[0]._index)
        for b in builders:
            result.merge(b)
        return result

    def validate(
        self,
        field_metadata: dict[str, Any] | None = None,
        validation_config: dict[str, Any] | None = None,
        synonyms: dict[str, str] | None = None,
    ) -> list[str]:
        """Validate the current query state against known field metadata.

        Field and attribute name checks are only performed when ``field_metadata``
        is provided. Without it, only structural checks (index, operator validity,
        name class values, etc.) are run.

        Args:
            field_metadata: Field metadata dict mapping field names to metadata
                objects (``processed_type``, ``summary``, ``constraint_enum``, etc.).
                Typically loaded from ``src/generated/field_meta.json`` in a
                generated project.
            validation_config: Optional override for validation rules (name classes,
                accession prefixes, etc.). Defaults to built-in rules.
            synonyms: Optional mapping of alias field names to canonical names.

        Returns:
            List of error strings. Empty list means the query is valid.
        """
        import json

        from cli_generator import validate_query_json

        result = validate_query_json(
            self.to_query_yaml(),
            json.dumps(field_metadata or {}),
            json.dumps(validation_config or {}),
            json.dumps(synonyms or {}),
        )

        try:
            return list(json.loads(result))
        except json.JSONDecodeError:
            return [result]

    def describe(
        self,
        field_metadata: dict[str, Any] | None = None,
        mode: str = "concise",
        report: "ReportBuilder | None" = None,
    ) -> str:
        """Get a human-readable description of this query.

        Args:
            field_metadata: Optional field metadata dictionary mapping field names to metadata
                objects with a ``display_name`` attribute. If not provided, canonical field
                names are used with underscores replaced by spaces.
            mode: Output format — ``"concise"`` for a one-line summary, ``"verbose"`` for
                a detailed breakdown.
            report: Optional :class:`ReportBuilder` to append a report description.
                When provided, the query description is followed by
                ``", visualised as <report description>"``.

        Returns:
            English prose description of the query.
        """
        import json

        from . import describe_query  # FFI call to Rust

        field_metadata_json = json.dumps(field_metadata or {})
        query_description = describe_query(
            self.to_query_yaml(),
            self.to_params_yaml(),
            field_metadata_json,
            mode,
        )

        if report is not None:
            report_phrase = report.describe()
            if report_phrase:
                # Strip trailing period from query part before appending
                base = query_description.rstrip(".")
                query_description = f"{base}, visualised as {report_phrase}."

        return query_description

    def snippet(
        self,
        languages: list[str] | None = None,
        *,
        call_type: str = "search",
        site_name: str = "site",
        sdk_name: str = "sdk",
        api_base: str = "",
        report: "ReportBuilder | None" = None,
    ) -> dict[str, str]:
        """Generate runnable code snippets for this query in one or more languages.

        Builds a :class:`QuerySnapshot` from the current builder state, passes it
        to the Rust snippet engine, and returns a mapping of language name to
        generated source code.

        Args:
            languages: Language codes to render.  Defaults to ``["python"]``.
                Supported values: ``"python"``, ``"r"``, ``"javascript"``, ``"cli"``.
            call_type: Which API call to illustrate.  One of:

                - ``"search"`` *(default)* — show a ``search()`` call
                - ``"count"`` — show a ``count()`` call
                - ``"report"`` — show a ``report(rb)`` call (requires ``report=``)
                - ``"positional"`` — show a ``positional()`` call
                - ``"search_batch"`` — show a ``search_batch()`` call
                - ``"count_batch"`` — show a ``count_batch()`` call

            site_name: Short identifier for the target site, e.g. ``"goat"``.
            sdk_name: Import name of the generated SDK package, e.g. ``"goat_sdk"``.
            api_base: Base URL of the API.
            report: :class:`ReportBuilder` instance, required when
                ``call_type="report"``.

        Returns:
            Dict mapping language name to generated source code string.
        """
        import json
        from typing import cast

        from . import render_snippet  # FFI call to Rust

        if languages is None:
            languages = ["python"]

        # Build filters, sorts, selections from builder state
        filters: list[tuple[str, str, str]] = []
        for attr in self._attributes:
            name: str = attr["name"]
            operator_str: str = str(attr.get("operator") or "")
            raw_value = attr.get("value")
            if raw_value is None:
                value_str = ""
            elif isinstance(raw_value, list):
                value_str = ", ".join(str(v) for v in raw_value)
            else:
                value_str = str(raw_value)
            filters.append((name, operator_str, value_str))

        sorts: list[tuple[str, str]] = []
        if self._sort_by is not None:
            sorts.append((self._sort_by, self._sort_order))

        selections = [f["name"] for f in self._fields]

        # Build the ReportSnapshot when a ReportBuilder is provided
        report_snapshot: dict[str, Any] | None = None
        if report is not None:
            import yaml  # type: ignore[import-untyped]

            rdoc: dict[str, Any] = yaml.safe_load(report.to_report_yaml()) or {}
            report_snapshot = {
                "report_type": str(rdoc.get("report", "")),
                "x": rdoc.get("x"),
                "y": rdoc.get("y") if isinstance(rdoc.get("y"), str) else None,
                "cat": rdoc.get("cat"),
                "rank": rdoc.get("rank"),
            }

        snapshot: dict[str, Any] = {
            "index": self._index,
            "taxa": self._taxa,
            "taxon_filter": self._taxon_filter_type,
            "rank": self._rank,
            "filters": filters,
            "sorts": sorts,
            "flags": [],
            "selections": selections,
            "traversal": None,
            "summaries": [],
            "call_type": call_type,
            "report": report_snapshot,
            "batch_queries": [],
            "positional": None,
        }

        result_json = render_snippet(
            json.dumps(snapshot),
            site_name,
            api_base,
            sdk_name,
            ",".join(languages),
        )

        return cast(dict[str, str], json.loads(result_json))


class ReportBuilder:
    """Builder for v3 ``/report`` POST body configuration.

    Constructs the ``report_yaml`` that controls how a report query is
    visualised.  Designed to be paired with a :class:`QueryBuilder`::

        rb = ReportBuilder("histogram").set_x("genome_size").set_rank("species")
        data = qb.report(rb)
    """

    def __init__(self, report_type: str) -> None:
        self._doc: dict[str, Any] = {"report": report_type}
        # Set by from_v2_url; takes priority in to_report_yaml() when set.
        self._report_yaml_override: str | None = None
        # Set by QueryBuilder.from_v2_url() for report URLs; used by run() when
        # no explicit query_builder is passed.
        self._embedded_query_builder: "QueryBuilder | None" = None
        # Set via set_display(); passed as the `display` key in the POST body.
        self._display: dict[str, Any] | str | None = None
        # Set via set_include_plot_spec(); requests a PlotSpec in the response.
        self._include_plot_spec: bool = False

    def set_x(self, field: str, opts: str = "") -> "ReportBuilder":
        """Set the X-axis field (histogram, scatter, arc reports)."""
        self._doc["x"] = field
        if opts:
            self._doc["x_opts"] = opts
        return self

    def set_y(self, field: str | list[str], opts: str = "") -> "ReportBuilder":
        """Set the Y-axis field or fields (scatter reports)."""
        self._doc["y"] = field
        if opts:
            self._doc["y_opts"] = opts
        return self

    def set_cat(self, field: str, opts: str = "") -> "ReportBuilder":
        """Set the category breakdown field."""
        self._doc["cat"] = field
        if opts:
            self._doc["cat_opts"] = opts
        return self

    def set_query(self, field: str) -> "ReportBuilder":
        """Set the query field (``countPerRank`` reports)."""
        self._doc["query"] = field
        return self

    def set_rank(self, rank: str) -> "ReportBuilder":
        """Set the taxonomic rank to aggregate at."""
        self._doc["rank"] = rank
        return self

    def set_ranks(self, ranks: list[str]) -> "ReportBuilder":
        """Set the list of taxonomic ranks (``countPerRank`` reports)."""
        self._doc["ranks"] = ranks
        return self

    def set_fields(self, fields: list[str]) -> "ReportBuilder":
        """Set additional fields to include in results."""
        self._doc["fields"] = fields
        return self

    def set_status_filter(self, value: str) -> "ReportBuilder":
        """Filter by assembly/sample status (e.g. ``"0"`` for any value)."""
        self._doc["status_filter"] = value
        return self

    def set_cat_rank(self, rank: str) -> "ReportBuilder":
        """Set the rank for category label aggregation."""
        self._doc["cat_rank"] = rank
        return self

    def set_collapse_monotypic(self, value: bool = True) -> "ReportBuilder":
        """Collapse monotypic nodes in tree reports."""
        self._doc["collapse_monotypic"] = value
        return self

    def set_preserve_rank(self, rank: str) -> "ReportBuilder":
        """Preserve this rank when collapsing monotypic nodes."""
        self._doc["preserve_rank"] = rank
        return self

    def set_count_rank(self, rank: str) -> "ReportBuilder":
        """Set the rank to count descendants at (tree reports)."""
        self._doc["count_rank"] = rank
        return self

    def set_location_field(self, field: str) -> "ReportBuilder":
        """Set the geographic location field (map reports)."""
        self._doc["location_field"] = field
        return self

    def set_hex_resolution(self, resolution: int) -> "ReportBuilder":
        """Set the geohash resolution for map reports (1–12)."""
        self._doc["hex_resolution"] = resolution
        return self

    def set_map_threshold(self, threshold: int) -> "ReportBuilder":
        """Set the max map points before switching to hexbin mode."""
        self._doc["map_threshold"] = threshold
        return self

    def set_scatter_threshold(self, threshold: int) -> "ReportBuilder":
        """Set the max scatter points before switching to binned mode."""
        self._doc["scatter_threshold"] = threshold
        return self

    # ── Arc report methods ────────────────────────────────────────────────────

    def set_feature(self, term: str) -> "ReportBuilder":
        """Set the feature filter for an arc report (the numerator condition).

        Args:
            term: Filter expression, e.g. ``\"genome_size>3000000000\"`` or
                a chain reference ``\"taxon_id=queryA.taxon_id\"``.
        """
        self._doc["feature"] = term
        return self

    def set_reference(self, term: str) -> "ReportBuilder":
        """Set the reference filter for an arc report (the denominator condition).

        Args:
            term: Filter expression, e.g. ``\"genome_size>0\"``.
        """
        self._doc["reference"] = term
        return self

    def set_context(self, term: str) -> "ReportBuilder":
        """Set the context filter for an arc report (enables ``arc2`` ratio).

        Args:
            term: Filter expression for the broader backdrop, e.g.
                ``\"assembly_level=Chromosome\"``.
        """
        self._doc["context"] = term
        return self

    def add_ring(
        self,
        feature_term: str,
        *,
        reference_term: str | None = None,
        label: str | None = None,
    ) -> "ReportBuilder":
        """Add a concentric ring to a multi-ring arc report.

        When rings are added, the ``feature`` key on the report is ignored and
        each ring's ``feature_term`` is used instead.  All rings share the
        outer ``reference`` filter unless overridden per-ring.

        Args:
            feature_term: Filter for the numerator of this ring, e.g.
                ``\"genome_size>0\"``.
            reference_term: Override the outer reference for this ring only.
                ``None`` uses the outer ``reference`` filter.
            label: Human-readable label for this ring in the response.
        """
        ring: dict[str, Any] = {"feature": feature_term}
        if reference_term is not None:
            ring["reference"] = reference_term
        if label is not None:
            ring["label"] = label
        self._doc.setdefault("rings", []).append(ring)
        return self

    def set_arc_ranks(self, ranks: list[str]) -> "ReportBuilder":
        """Run the same feature/reference arc once per taxonomic rank.

        Each rank becomes one concentric ring.  Mutually exclusive with
        :meth:`add_ring`.

        Args:
            ranks: List of rank names, e.g.
                ``[\"genus\", \"family\", \"order\"]``.
        """
        self._doc["ranks"] = list(ranks)
        return self

    def set_axis_boundaries(
        self, axis_role: str, boundaries: list[float | str], *, labels: list[str] | None = None
    ) -> "ReportBuilder":
        """Set custom boundaries for a histogram axis (x, y, or cat).

        For numeric axes, boundaries define explicit breakpoints. For date axes,
        provide ISO 8601 date strings or interval names (\"week\", \"month\", \"quarter\").

        Args:
            axis_role: Axis to configure — one of ``"x"``, ``"y"``, or ``"cat"``.
            boundaries: For numeric: list of floats in ascending order.
                For date: list of ISO 8601 strings or interval names.
            labels: Optional custom bucket labels. Count must equal ``len(boundaries) - 1``
                for numeric boundaries, or the number of resolved intervals for dates.

        Returns:
            Self for chaining.
        """
        key = f"{axis_role}_opts"
        if key not in self._doc:
            self._doc[key] = {}
        self._doc[key]["boundaries"] = boundaries
        if labels is not None:
            self._doc[key]["labels"] = labels
        return self

    def set_axis_date_intervals(self, axis_role: str, intervals: list[str]) -> "ReportBuilder":
        """Set date-based intervals for a date-scaled axis.

        Convenience method for setting standard calendar intervals on a date axis.
        Intervals are expanded server-side to boundaries for the current time window.

        Args:
            axis_role: Axis to configure — one of ``"x"``, ``"y"``, or ``"cat"``.
            intervals: List of interval names, e.g. ``["week", "month", "quarter"]``.

        Returns:
            Self for chaining.
        """
        key = f"{axis_role}_opts"
        if key not in self._doc:
            self._doc[key] = {}
        self._doc[key]["boundaries"] = {"intervals": intervals}
        return self

    def set_display(self, value: "dict[str, Any] | str") -> "ReportBuilder":
        """Set display/presentation options for this report.

        Accepts either a dict of display options or a YAML string. The value is
        passed as the ``display`` field in the API request and returned in the
        response unchanged. Rendering is always client-side.

        Args:
            value: Dict or YAML string with display options such as ``title``,
                ``width``, ``height``, ``color_scheme``, ``x_label``, etc.
        """
        self._display = value
        return self

    def set_include_plot_spec(self, value: bool = True) -> "ReportBuilder":
        """Request a ``plot_spec`` field in the API response.

        When ``True``, the server builds and returns a fully-resolved
        :class:`PlotSpec` object alongside the raw report data.  Pass the
        result to :func:`plot_spec_to_vega_lite` to produce a Vega-Lite spec.

        Args:
            value: Whether to include the plot spec (default: ``True``).
        """
        self._include_plot_spec = value
        return self

    def to_report_yaml(self) -> str:
        """Return the report configuration as a YAML string."""
        if self._report_yaml_override is not None:
            return self._report_yaml_override

        import yaml  # type: ignore[import-untyped]

        return yaml.safe_dump(self._doc, sort_keys=False)

    def describe(self) -> str:
        """Return a short English description of this report configuration.

        Returns a phrase suitable for embedding in prose, e.g.:
        ``"a histogram of genome size by species rank"``.

        Returns:
            Short description string, or an empty string if the report type
            is unrecognised.
        """
        from . import describe_report_yaml  # FFI call to Rust

        return describe_report_yaml(self.to_report_yaml())

    def validate(self, field_meta: dict[str, Any] | None = None) -> list[str]:
        """Return a list of validation errors. An empty list means the report is valid.

        Args:
            field_meta: Optional mapping of field names to metadata dicts.
                When provided, axis field names are checked against known fields.

        Returns:
            List of error strings.
        """
        import json

        from cli_generator import validate_report_yaml

        field_meta_json = json.dumps(field_meta or {})
        return list(json.loads(validate_report_yaml(self.to_report_yaml(), field_meta_json)))

    def run(
        self,
        query_builder: "QueryBuilder | None" = None,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3",
    ) -> Any:
        """Execute this report against a :class:`QueryBuilder`'s query.

        When this ``ReportBuilder`` was created via
        :meth:`QueryBuilder.from_v2_url`, the query is embedded and
        ``query_builder`` may be omitted.

        Args:
            query_builder: Query that defines the search scope.  May be
                ``None`` when the builder was produced by
                :meth:`QueryBuilder.from_v2_url`.
            api_base: Base URL of the API.
            api_version: API version string (default: ``"v3"``).

        Returns:
            Raw ``report`` dict from the response.

        Raises:
            ValueError: When ``query_builder`` is ``None`` and no embedded
                query is available.
        """
        qb = query_builder if query_builder is not None else self._embedded_query_builder
        if qb is None:
            raise ValueError(
                "run() requires a QueryBuilder argument or a ReportBuilder " "created via QueryBuilder.from_v2_url()"
            )
        return qb.report(self, api_base=api_base, api_version=api_version)


def plot_spec_to_vega_lite(spec: dict[str, Any]) -> dict[str, Any]:
    """Convert a ``PlotSpec`` dict (from the API ``plot_spec`` field) to a Vega-Lite v5 spec.

    The returned dict can be passed directly to any Vega-Lite renderer, e.g.
    ``altair.Chart.from_dict()``.

    Args:
        spec: A ``PlotSpec`` dict from the API response ``plot_spec`` field.
            Typically obtained via
            ``ReportBuilder.set_include_plot_spec().run(qb)``.

    Returns:
        Vega-Lite v5 JSON-compatible dict.
    """
    display: dict[str, Any] = spec.get("display") or {}
    base: dict[str, Any] = {
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "width": display.get("width", 600),
        "height": display.get("height", 400),
        "config": {
            "axis": {
                "labelFontSize": display.get("font_size", 12),
                "titleFontSize": display.get("font_size", 12),
            },
            "legend": {"labelFontSize": display.get("font_size", 12)},
        },
    }
    title = display.get("title")
    if title is not None:
        base["title"] = title

    report_type: str = spec.get("report_type", "")
    x: dict[str, Any] = spec.get("x") or {}
    y: dict[str, Any] = spec.get("y") or {}
    data_vals: dict[str, Any] = spec.get("data") or {}

    if report_type == "histogram":
        hist = display.get("histogram") or {}
        base.update(
            {
                "data": {"values": data_vals.get("buckets", [])},
                "mark": {"type": "bar"},
                "encoding": {
                    "x": {
                        "field": "key",
                        "type": "quantitative",
                        "scale": {"type": "log" if x.get("scale") == "log10" else "linear"},
                        "axis": {"title": x.get("label") or x.get("field", "")},
                    },
                    "y": {
                        "field": "doc_count",
                        "type": "quantitative",
                        "scale": {"type": "log" if hist.get("y_scale") == "log10" else "linear"},
                        "axis": {"title": display.get("y_label", "Count")},
                    },
                },
            }
        )
    elif report_type == "scatter":
        base.update(
            {
                "data": {"values": data_vals.get("cells", [])},
                "mark": "point",
                "encoding": {
                    "x": {
                        "field": "x",
                        "type": "quantitative",
                        "scale": {"type": "log" if x.get("scale") == "log10" else "linear"},
                        "axis": {"title": x.get("label") or x.get("field", "")},
                    },
                    "y": {
                        "field": "y",
                        "type": "quantitative",
                        "scale": {"type": "log" if y.get("scale") == "log10" else "linear"},
                        "axis": {"title": y.get("label") or y.get("field", "")},
                    },
                },
            }
        )
    elif report_type in ("count_per_rank", "sources"):
        base.update(
            {
                "data": {"values": data_vals.get("buckets", [])},
                "mark": "bar",
                "encoding": {
                    "y": {
                        "field": x.get("field", "rank"),
                        "type": "nominal",
                        "axis": {"title": x.get("label") or x.get("field", "")},
                    },
                    "x": {"field": "count", "type": "quantitative"},
                },
            }
        )

    return base


def local_plot_spec(
    content: str,
    report_type: str = "histogram",
    column_map: dict[str, str] | None = None,
    display: dict[str, Any] | None = None,
    delimiter: str = "\t",
) -> dict[str, Any]:
    """Build a PlotSpec from local delimited text content without an API call.

    Auto-detects column types: columns where every non-empty value is numeric
    are stored as numbers; everything else is stored as strings.

    Args:
        content: Full text of the TSV/CSV file.
        report_type: One of ``"histogram"``, ``"scatter"``, or ``"bar"``.
        column_map: Mapping of axis roles to column names, e.g.
            ``{"x": "genome_size", "y": "c_value"}``.  Pass ``None`` or an
            empty dict to use positional defaults (first column → x, second
            column → y).
        display: Display options dict (title, width, height, etc.).  ``None``
            means defaults.
        delimiter: Field separator: ``"\\t"`` for TSV, ``","`` for CSV.
            Defaults to ``"\\t"``.

    Returns:
        PlotSpec dict compatible with :func:`plot_spec_to_vega_lite`.

    Raises:
        ValueError: When the report type is unknown, a required column is
            missing, or a numeric-axis column contains non-numeric data.
    """
    import json

    from cli_generator import local_plot_spec_json as _local_plot_spec_json

    col_map = column_map or {}
    display_json = json.dumps(display) if display else "{}"
    result = _local_plot_spec_json(content, report_type, json.dumps(col_map), display_json, delimiter)
    parsed: dict[str, Any] = json.loads(result)
    if "error" in parsed:
        raise ValueError(parsed["error"])
    return parsed


def merge_annotations(
    plot_spec: dict[str, Any],
    annotations: list[dict[str, Any]],
    join_key: str,
) -> dict[str, Any]:
    """Merge annotation dicts into ``plot_spec.data`` rows by a shared key.

    For each row in ``plot_spec["data"]["rows"]`` that has a value for
    ``join_key`` matching an annotation entry, the annotation's fields are
    added to the row (annotation fields take precedence on key collision).
    Rows with no matching annotation are left unchanged.

    The ``plot_spec`` dict is modified in-place and also returned.

    Args:
        plot_spec: A PlotSpec dict (from the API or from :func:`local_plot_spec`).
        annotations: A list of dicts, each containing at least ``join_key``
            and any additional fields to add.
        join_key: The column name used to match rows to annotation entries.

    Returns:
        The modified ``plot_spec`` dict (same object).
    """
    annotation_index: dict[Any, dict[str, Any]] = {entry[join_key]: entry for entry in annotations if join_key in entry}
    data = plot_spec.get("data") or {}
    rows: list[dict[str, Any]] = data.get("rows", [])
    for row in rows:
        key_val = row.get(join_key)
        if key_val is not None and key_val in annotation_index:
            row.update(annotation_index[key_val])
    return plot_spec
