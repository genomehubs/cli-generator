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

from typing import Any

# ── Public re-export ──────────────────────────────────────────────────────────

__all__ = ["QueryBuilder"]


class QueryBuilder:
    """Accumulate a genomehubs ``SearchQuery`` incrementally.

    Each ``set_*`` / ``add_*`` method returns ``self`` to support method
    chaining as well as the staged MCP-tool calling pattern.

    Args:
        index: Index to search — one of ``"taxon"``, ``"assembly"``,
            ``"sample"``.  More indexes may be available depending on the
            site.
        validation_level: Validation mode for ``validate()``:
            - ``"full"`` (default): Attempts to fetch metadata from v3 API endpoints
              (gracefully handles 404s if not yet deployed). Falls back to local files.
            - ``"partial"``: Uses only embedded files (no API fetch). Recommended
              until v3 API is available.
        api_base: Base URL for API metadata endpoints (v3+). Defaults to
            ``"https://genomehubs.org"``.
    """

    def __init__(
        self,
        index: str,
        validation_level: str = "full",
        api_base: str = "https://genomehubs.org",
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
        # QueryParams
        self._size: int = 10
        self._page: int = 1
        self._sort_by: str | None = None
        self._sort_order: str = "asc"
        self._include_estimates: bool = True
        self._tidy: bool = False
        self._taxonomy: str = "ncbi"
        # Validation options
        self._validation_level: str = validation_level
        self._api_base: str = api_base

    # ── Identifiers ──────────────────────────────────────────────────────────

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

    def to_tidy_records(self, records: list[dict[str, Any]] | str) -> list[dict[str, Any]]:
        """Reshape flat records from ``parse_search_json`` into long/tidy format.

        Each flat record is exploded so that every bare field becomes its own
        row with columns ``"field"``, ``"value"``, ``"source"``, and any identity
        columns (``taxon_id``, ``scientific_name``, ``taxon_rank``, …) present in
        the source record.

        Explicitly-requested modifier columns (from ``field:modifier`` requests,
        e.g. ``assembly_span__min``) are emitted as separate rows with ``"field"``
        set to ``"{bare}:{modifier}"`` and ``"source"`` as ``None``.

        This is the natural input for ``pandas.melt`` or R's ``tidyr::pivot_longer``.

        Args:
            records: Either the JSON string from ``parse_search_json`` or an
                already-parsed list of flat record dicts.

        Returns:
            List of dicts in tidy (long) format.
        """
        import json

        from . import to_tidy_records as _to_tidy_records  # FFI call to Rust

        records_json = records if isinstance(records, str) else json.dumps(records)
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

    # ── Serialisation ────────────────────────────────────────────────────────

    def to_query_yaml(self) -> str:
        """Serialise the query into a YAML string for :func:`build_url`."""
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

        return yaml.safe_dump(doc, sort_keys=False)

    def to_params_yaml(self) -> str:
        """Serialise the execution parameters into a YAML string."""
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

        return yaml.safe_dump(doc, sort_keys=False)

    # ── URL + API calls ───────────────────────────────────────────────────────

    def to_url(
        self,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v2",
        endpoint: str = "search",
    ) -> str:
        """Build the full API URL for this query without making a network call.

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

    def count(
        self,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v2",
    ) -> int:
        """Fetch the count of records matching this query.

        Args:
            api_base: Base URL of the API.
            api_version: API version string.

        Returns:
            Number of matching records.
        """
        import json
        import urllib.request

        from . import parse_response_status

        counter = QueryBuilder(self._index)
        counter.merge(self)
        counter.set_size(0)
        url = counter.to_url(api_base, api_version, "search")
        with urllib.request.urlopen(url) as resp:
            body_text = resp.read().decode("utf-8")
        status = json.loads(parse_response_status(body_text))
        return int(status.get("hits") or 0)

    def search(
        self,
        format: str = "json",
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v2",
    ) -> Any:
        """Fetch results for this query.

        Args:
            format: Response format — ``"json"`` (default) or ``"tsv"``.
            api_base: Base URL of the API.
            api_version: API version string.

        Returns:
            Parsed JSON (dict) for ``format="json"``; raw text for ``"tsv"``.
        """
        import json
        import urllib.request

        url = self.to_url(api_base, api_version, "search")
        headers = {
            "json": "application/json",
            "tsv": "text/tab-separated-values",
        }.get(format, "application/json")
        req = urllib.request.Request(url, headers={"Accept": headers})
        with urllib.request.urlopen(req) as resp:
            raw = resp.read().decode()
        if format == "json":
            return json.loads(raw)
        return raw

    def search_all(
        self,
        max_records: int | None = None,
        api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v2",
    ) -> list[dict[str, Any]]:
        """Fetch all matching records using cursor-based pagination.

        Uses the ``/searchPaginated`` endpoint in chunks of 1 000 records per
        page.  Pagination continues until all pages are retrieved or
        *max_records* is reached.

        Args:
            max_records: Maximum total records to return.  ``None`` means no
                limit.
            api_base: Base URL of the API.
            api_version: API version string.

        Returns:
            List of flat record dicts in the same format as
            :func:`~cli_generator.parse_search_json` output.
        """
        import json
        import urllib.parse
        import urllib.request

        from . import parse_paginated_json

        CHUNK_SIZE = 1_000
        cap: float = float("inf") if max_records is None else float(max_records)
        all_records: list[dict[str, Any]] = []
        search_after: list[Any] | None = None

        while True:
            base_url = self.to_url(api_base, api_version, "searchPaginated")
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

    def describe(self, field_metadata: dict[str, Any] | None = None, mode: str = "concise") -> str:
        """Get a human-readable description of this query.

        Args:
            field_metadata: Optional field metadata dictionary mapping field names to metadata
                objects with a `display_name` attribute. If not provided, canonical field
                names are used with underscores replaced by spaces.
            mode: Output format — ``"concise"`` for a one-line summary, ``"verbose"`` for
                a detailed breakdown.

        Returns:
            English prose description of the query.

        Example::

            qb = QueryBuilder("taxon").add_attribute("genome_size", ">=", "1G")
            print(qb.describe())
            # Output: "Search for taxa, filtered to genome size >= 1000000000, returning all fields."

            print(qb.describe(mode="verbose"))
            # Output: "Search for taxa in the database.
            #          Filters applied:
            #            • genome size >= 1 gigabyte
            #          ..."
        """
        import json

        from . import describe_query  # FFI call to Rust

        # Convert field metadata to JSON for FFI
        field_metadata_json = json.dumps(field_metadata or {})

        return describe_query(
            self.to_query_yaml(),
            self.to_params_yaml(),
            field_metadata_json,
            mode,
        )

    def snippet(
        self,
        languages: list[str] | None = None,
        *,
        site_name: str = "site",
        sdk_name: str = "sdk",
        api_base: str = "",
    ) -> dict[str, str]:
        """Generate runnable code snippets for this query in one or more languages.

        Builds a :class:`QuerySnapshot` from the current builder state, passes it
        to the Rust snippet engine, and returns a mapping of language name to
        generated source code.

        Args:
            languages: Language codes to render.  Defaults to ``["python"]``.
                Additional languages (``"r"``, ``"javascript"``) become available
                as their templates are added in later phases.
            site_name: Short identifier for the target site, e.g. ``"goat"``.
                Used as a comment label in the generated snippet.
            sdk_name: Import name of the generated SDK package, e.g.
                ``"goat_sdk"``.  Appears in the ``import`` statement.
            api_base: Base URL of the API, e.g.
                ``"https://goat.genomehubs.org/api"``.

        Returns:
            Dict mapping language name to generated source code string, e.g.
            ``{"python": "import goat_sdk as sdk\\n..."}``.

        Example::

            qb = (
                QueryBuilder("taxon")
                .add_attribute("genome_size", operator=">=", value="1000000000")
                .add_field("organism_name")
            )
            code = qb.snippet(site_name="goat", sdk_name="goat_sdk")["python"]
            print(code)
            # import goat_sdk as sdk
            # qb = sdk.QueryBuilder("taxon")
            # qb.add_attribute("genome_size", operator=">=", value="1000000000")
            # ...
        """
        import json
        from typing import cast

        from . import render_snippet  # FFI call to Rust

        if languages is None:
            languages = ["python"]

        # Build a QuerySnapshot-compatible dict from internal builder state.
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

        snapshot = {
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
        }

        result_json = render_snippet(
            json.dumps(snapshot),
            site_name,
            api_base,
            sdk_name,
            ",".join(languages),
        )

        return cast(dict[str, str], json.loads(result_json))
