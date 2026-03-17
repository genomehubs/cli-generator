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
    """

    def __init__(self, index: str) -> None:
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

        Args:
            name: Field name, e.g. ``"genome_size"``.
            modifiers: Summary modifiers to include as separate columns,
                e.g. ``["min", "max"]``.
        """
        entry: dict[str, Any] = {"name": name}
        if modifiers:
            entry["modifier"] = list(modifiers)
        self._fields.append(entry)
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
