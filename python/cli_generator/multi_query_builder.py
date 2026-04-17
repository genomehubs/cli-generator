"""Batch query execution via the genomehubs ``/msearch`` endpoint.

``MultiQueryBuilder`` collects a list of :class:`~cli_generator.QueryBuilder`
instances that share common execution parameters (fields, size, sort,
taxonomy, include_estimates) and executes them as a single POST request to
``/msearch``, automatically splitting into batches of up to 100 searches when
needed.

Typical usage::

    from cli_generator import QueryBuilder
    from cli_generator.multi_query_builder import MultiQueryBuilder

    mq = MultiQueryBuilder("taxon")
    mq.set_size(100)
    mq.set_fields(["genome_size", "assembly_level"])

    mq.add_query(QueryBuilder("taxon").set_taxa(["Caenorhabditis"], filter_type="tree"))
    mq.add_query(QueryBuilder("taxon").set_taxa(["Homo sapiens"]))

    results = mq.search()  # list[list[dict[str, Any]]]
    # results[0] → flat records for Caenorhabditis query
    # results[1] → flat records for Homo sapiens query
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .query import QueryBuilder

# Keys that must be uniform across the whole batch (hard error on per-query divergence).
_FROZEN_KEYS: frozenset[str] = frozenset({"include_estimates", "taxonomy"})

# Keys that are shared/overridable (warn on per-query divergence).
_OVERRIDABLE_KEYS: frozenset[str] = frozenset({"size", "sort"})

# Keys forbidden in the shared: section of a full YAML file.
_FORBIDDEN_IN_SHARED: frozenset[str] = frozenset({"taxa", "assemblies", "samples"})

# Maximum searches per /msearch POST call (API hard limit).
_MSEARCH_BATCH_SIZE: int = 100


class MultiQueryBuilder:
    """Batch query builder that issues multiple searches via ``/msearch``.

    Shared parameters (fields, size, sort, taxonomy, include_estimates) are set
    once on the ``MultiQueryBuilder`` and applied to every search.  Per-query
    variation is supported only for filters (taxa, rank, attributes, etc.);
    ``include_estimates`` and ``taxonomy`` cannot vary per-query.

    Args:
        index: The API index to query — one of ``"taxon"``, ``"assembly"``,
            ``"sample"``.
    """

    def __init__(self, index: str) -> None:
        self._index = index
        self._queries: list[QueryBuilder] = []
        # Shared execution params — mirror QueryBuilder defaults.
        self._size: int = 10
        self._sort_by: str | None = None
        self._sort_order: str = "asc"
        self._include_estimates: bool = True
        self._taxonomy: str = "ncbi"
        self._fields: list[str | dict[str, Any]] = []
        self._names: list[str] = []
        self._ranks: list[str] = []

    # ── Shared param setters ──────────────────────────────────────────────────

    def set_size(self, size: int) -> "MultiQueryBuilder":
        """Set the number of records to return per query (shared)."""
        self._size = size
        return self

    def set_sort(self, field: str, order: str = "asc") -> "MultiQueryBuilder":
        """Sort results by ``field`` in ``order`` (shared)."""
        self._sort_by = field
        self._sort_order = order
        return self

    def set_include_estimates(self, value: bool) -> "MultiQueryBuilder":
        """Control whether estimated values are included (frozen — applies to all queries)."""
        self._include_estimates = value
        return self

    def set_taxonomy(self, taxonomy: str) -> "MultiQueryBuilder":
        """Set the taxonomy source (frozen — applies to all queries)."""
        self._taxonomy = taxonomy
        return self

    def set_fields(self, fields: list[str | dict[str, Any]]) -> "MultiQueryBuilder":
        """Set the field names to return in every result row (shared)."""
        self._fields = list(fields)
        return self

    def set_names(self, name_classes: list[str]) -> "MultiQueryBuilder":
        """Set additional name class columns (shared)."""
        self._names = list(name_classes)
        return self

    def set_ranks(self, ranks: list[str]) -> "MultiQueryBuilder":
        """Set lineage rank columns (shared)."""
        self._ranks = list(ranks)
        return self

    # ── Query management ──────────────────────────────────────────────────────

    def add_query(
        self,
        qb: QueryBuilder,
        warn_on_param_divergence: bool = True,
    ) -> "MultiQueryBuilder":
        """Add a :class:`QueryBuilder` to the batch.

        Raises :exc:`ValueError` if ``qb`` sets ``include_estimates`` or
        ``taxonomy`` to a value different from this builder's frozen params.

        Args:
            qb: Query to add.  Its index must match this builder's index.
            warn_on_param_divergence: Emit a warning when ``qb`` has a
                different ``size`` or ``sort`` from the shared value.  Set
                ``False`` to suppress.
        """
        if qb._index != self._index:
            raise ValueError(f"Cannot add a '{qb._index}' query to a MultiQueryBuilder for '{self._index}'")
        # Frozen-param validation.
        if qb._include_estimates != self._include_estimates:
            raise ValueError(
                f"'include_estimates' cannot diverge per-query: "
                f"MultiQueryBuilder has {self._include_estimates}, "
                f"query has {qb._include_estimates}"
            )
        if qb._taxonomy != self._taxonomy:
            raise ValueError(
                f"'taxonomy' cannot diverge per-query: "
                f"MultiQueryBuilder has '{self._taxonomy}', "
                f"query has '{qb._taxonomy}'"
            )
        # Overridable-param warnings.
        if warn_on_param_divergence:
            if qb._size != 10 and qb._size != self._size:
                import warnings

                warnings.warn(
                    f"Query {len(self._queries)}: size={qb._size} overrides " f"MultiQueryBuilder size={self._size}",
                    stacklevel=2,
                )
            if qb._sort_by is not None and qb._sort_by != self._sort_by:
                import warnings

                warnings.warn(
                    f"Query {len(self._queries)}: sort={qb._sort_by}:{qb._sort_order} "
                    f"overrides MultiQueryBuilder sort={self._sort_by}:{self._sort_order}",
                    stacklevel=2,
                )
        self._queries.append(qb)
        return self

    def from_file(
        self,
        path: str | Path,
        taxon_filter: str = "name",
        suppress_divergence_warnings: bool = False,
    ) -> "MultiQueryBuilder":
        """Populate the batch from a file.

        Supports three formats (auto-detected):

        - **Full search YAML** — top-level ``queries:`` key; optional
          ``shared:`` section sets defaults.
        - **Patch YAML array** — sequence of dicts with ``taxon``, ``rank``,
          and/or ``filter`` keys.
        - **Bare taxon list** — one taxon name per non-empty line.

        Args:
            path: Path to the file.
            taxon_filter: How to wrap bare taxon names — ``"name"``,
                ``"tree"``, or ``"lineage"``.
            suppress_divergence_warnings: If ``True``, suppress per-query
                size/sort divergence warnings.

        Returns:
            ``self`` for chaining.
        """
        import yaml  # type: ignore[import-untyped]

        content = Path(path).read_text(encoding="utf-8").strip()
        parsed = yaml.safe_load(content)

        if isinstance(parsed, dict) and "queries" in parsed:
            self._load_from_full_yaml(parsed, taxon_filter, suppress_divergence_warnings)
        elif isinstance(parsed, list):
            self._load_from_patch_array(parsed, taxon_filter, suppress_divergence_warnings)
        else:
            self._load_from_bare_list(content, taxon_filter)
        return self

    def _load_from_full_yaml(
        self,
        doc: dict[str, Any],
        taxon_filter: str,
        suppress_divergence_warnings: bool,
    ) -> None:
        shared: dict[str, Any] = doc.get("shared", {})
        bad = set(shared) & _FORBIDDEN_IN_SHARED
        if bad:
            raise ValueError(f"{sorted(bad)} are not valid in shared:; " f"set them per-query under queries:")
        # Apply shared params to self (lower precedence than already-set values
        # only if the attribute is still at its default).
        if "size" in shared:
            self.set_size(int(shared["size"]))
        if "sort" in shared:
            field, _, order = str(shared["sort"]).partition(":")
            self.set_sort(field, order or "asc")
        if "include_estimates" in shared:
            self.set_include_estimates(bool(shared["include_estimates"]))
        if "taxonomy" in shared:
            self.set_taxonomy(str(shared["taxonomy"]))
        if "fields" in shared:
            self.set_fields(shared["fields"])
        if "names" in shared:
            self.set_names(shared["names"])
        if "ranks" in shared:
            self.set_ranks(shared["ranks"])

        patches: list[Any] = doc.get("queries", [])
        self._load_from_patch_array(patches, taxon_filter, suppress_divergence_warnings)

    def _load_from_patch_array(
        self,
        patches: list[Any],
        taxon_filter: str,
        suppress_divergence_warnings: bool,
    ) -> None:
        for entry in patches:
            if not isinstance(entry, dict):
                raise ValueError(f"Each entry in the batch file must be a mapping, got: {entry!r}")
            # Frozen-key guard: per-query entries must not set frozen params.
            for fk in _FROZEN_KEYS:
                if fk in entry:
                    raise ValueError(
                        f"'{fk}' cannot be set per-query; " f"set it in shared: or on the MultiQueryBuilder"
                    )
            qb = QueryBuilder(self._index)
            qb.set_include_estimates(self._include_estimates)
            qb.set_taxonomy(self._taxonomy)

            if "taxon" in entry:
                qb.set_taxa([str(entry["taxon"])], filter_type=taxon_filter)
            if "rank" in entry:
                qb.set_rank(str(entry["rank"]))
            if "filter" in entry:
                _apply_filter_entry(qb, entry["filter"])

            warn = not suppress_divergence_warnings
            self.add_query(qb, warn_on_param_divergence=warn)

    def _load_from_bare_list(self, content: str, taxon_filter: str) -> None:
        for line in content.splitlines():
            name = line.strip()
            if not name:
                continue
            qb = QueryBuilder(self._index)
            qb.set_include_estimates(self._include_estimates)
            qb.set_taxonomy(self._taxonomy)
            qb.set_taxa([name], filter_type=taxon_filter)
            self._queries.append(qb)

    # ── Execution ─────────────────────────────────────────────────────────────

    def search(
        self,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v2",
    ) -> list[list[dict[str, Any]]]:
        """Execute all queries simultaneously via ``/msearch``.

        Automatically batches into groups of up to 100 searches (the API
        limit) and reassembles results in input order.

        Args:
            api_base: Base URL of the API.
            api_version: API version string.

        Returns:
            List of per-query flat record lists.  ``results[i]`` corresponds
            to ``queries[i]``.
        """
        import json
        import urllib.request

        from . import parse_msearch_json

        if not self._queries:
            return []

        # Build the flat fields string from self._fields.
        fields_str = _fields_to_str(self._fields)
        # Build the names/ranks strings from shared params.
        names_str = ",".join(self._names) if self._names else ""
        ranks_str = ",".join(self._ranks) if self._ranks else ""

        all_results: list[list[dict[str, Any]]] = [[] for _ in self._queries]

        for batch_start in range(0, len(self._queries), _MSEARCH_BATCH_SIZE):
            chunk = self._queries[batch_start : batch_start + _MSEARCH_BATCH_SIZE]

            searches: list[dict[str, Any]] = []
            for qb in chunk:
                # Extract the raw query string from the URL.
                url = qb.to_url(api_base, api_version, "search")
                query_param = _extract_query_param(url)
                search_obj: dict[str, Any] = {
                    "query": query_param,
                    "result": self._index,
                    "taxonomy": self._taxonomy,
                    "limit": self._size,
                    "includeEstimates": self._include_estimates,
                }
                if fields_str:
                    search_obj["fields"] = fields_str
                if names_str:
                    search_obj["names"] = names_str
                if ranks_str:
                    search_obj["ranks"] = ranks_str
                if self._sort_by:
                    search_obj["sortBy"] = self._sort_by
                    search_obj["sortOrder"] = self._sort_order
                searches.append(search_obj)

            body = json.dumps({"searches": searches}).encode()
            req = urllib.request.Request(
                f"{api_base}/{api_version}/msearch",
                data=body,
                headers={
                    "Content-Type": "application/json",
                    "Accept": "application/json",
                },
                method="POST",
            )
            with urllib.request.urlopen(req) as resp:
                raw: str = resp.read().decode()

            batch_result: dict[str, Any] = json.loads(parse_msearch_json(raw))
            for i, query_result in enumerate(batch_result.get("results", [])):
                all_results[batch_start + i] = query_result.get("records", [])

        return all_results

    def __len__(self) -> int:
        return len(self._queries)

    def __repr__(self) -> str:
        return f"MultiQueryBuilder({self._index!r}, queries={len(self._queries)})"


# ── Module-level convenience loader ──────────────────────────────────────────


def from_file(
    index: str,
    path: str | Path,
    taxon_filter: str = "name",
    suppress_divergence_warnings: bool = False,
) -> MultiQueryBuilder:
    """Create a :class:`MultiQueryBuilder` populated from a file.

    Convenience wrapper — equivalent to::

        mq = MultiQueryBuilder(index)
        mq.from_file(path, taxon_filter=taxon_filter)

    Args:
        index: The API index (``"taxon"``, ``"assembly"``, or ``"sample"``).
        path: Path to the batch file.
        taxon_filter: How to wrap bare taxon names.
        suppress_divergence_warnings: Suppress size/sort divergence warnings.
    """
    mq = MultiQueryBuilder(index)
    mq.from_file(path, taxon_filter=taxon_filter, suppress_divergence_warnings=suppress_divergence_warnings)
    return mq


# ── Private helpers ───────────────────────────────────────────────────────────


def _apply_filter_entry(qb: QueryBuilder, filter_val: Any) -> None:
    """Apply a ``filter:`` value from a YAML patch dict to ``qb``."""
    if isinstance(filter_val, str):
        # e.g. "assembly_level = chromosome" — pass as raw query fragment
        # by adding it as a generic attribute (operator/value unparsed here;
        # the API accepts it as part of the query string directly via
        # adding_attribute with raw expression style).
        # For simplicity: treat as "exists"-style via set_attributes raw str.
        # Actually: just store for to_query_yaml as a raw expression.
        # The cleanest way is to parse the string into (field, op, value).
        parts = filter_val.split(maxsplit=2)
        if len(parts) == 3:
            field, op, value = parts
            qb.add_attribute(field, operator=_normalise_op(op), value=value)
        elif len(parts) == 2:
            field, op = parts
            qb.add_attribute(field, operator=_normalise_op(op))
    elif isinstance(filter_val, list):
        for item in filter_val:
            _apply_filter_entry(qb, item)


def _normalise_op(op: str) -> str:
    return {
        "==": "eq",
        "=": "eq",
        "!=": "ne",
        ">": "gt",
        ">=": "ge",
        "<": "lt",
        "<=": "le",
    }.get(op, op)


def _fields_to_str(fields: list[str | dict[str, Any]]) -> str:
    """Flatten ``fields`` list to a comma-separated string for the API."""
    parts: list[str] = []
    for f in fields:
        if isinstance(f, str):
            parts.append(f)
        elif isinstance(f, dict):
            name = str(f.get("name", ""))
            mods: list[str] = f.get("modifier", [])
            if mods:
                parts.extend(f"{name}:{m}" for m in mods)
            else:
                parts.append(name)
    return ",".join(parts)


def _extract_query_param(url: str) -> str:
    """Extract the decoded value of the ``query=`` parameter from a URL."""
    import urllib.parse

    parsed = urllib.parse.urlparse(url)
    qs = urllib.parse.parse_qs(parsed.query, keep_blank_values=True)
    values = qs.get("query", [""])
    return values[0] if values else ""
