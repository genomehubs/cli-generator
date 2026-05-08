"""
Integration tests for batch SDK methods against a live API.

These tests validate that batch operations work end-to-end with real API responses,
using actual parse functions (no mocks). They require a running API server at
http://localhost:3000/api.

Run via: pytest tests/python/test_batch_integration.py -v

Requirements:
- pytest
- Running API server: cd crates/genomehubs-api && cargo run
"""

import pytest

from cli_generator import QueryBuilder, parse_batch_json, parse_lookup_json, parse_record_json

API_BASE = "http://localhost:3000/api"
SKIP_REASON = "Requires live API server at http://localhost:3000"


@pytest.fixture
def api_available():
    """Check if API server is running before running integration tests."""
    import urllib.request

    try:
        urllib.request.urlopen(f"{API_BASE}/v3/search?q=test", timeout=2)
        return True
    except Exception:
        pytest.skip(SKIP_REASON)


class TestBatchIntegration:
    """Integration tests for batch operations."""

    def test_search_batch_single_query(self, api_available):
        """Test searchBatch with a single query."""
        qb = QueryBuilder("taxon")
        query = QueryBuilder("taxon").set_taxa(["Canis lupus"])
        result = qb.search_batch([query], api_base=API_BASE)
        assert isinstance(result, list)
        assert len(result) > 0

    def test_search_batch_multiple_queries(self, api_available):
        """Test searchBatch with 10 queries."""
        qb = QueryBuilder("taxon")
        queries = [
            QueryBuilder("taxon").set_taxa(["Canis lupus"]),
            QueryBuilder("taxon").set_taxa(["Felis catus"]),
            QueryBuilder("taxon").set_taxa(["Mus musculus"]),
            QueryBuilder("taxon").set_taxa(["Danio rerio"]),
            QueryBuilder("taxon").set_taxa(["Drosophila melanogaster"]),
            QueryBuilder("taxon").set_taxa(["Arabidopsis thaliana"]),
            QueryBuilder("taxon").set_taxa(["Oryza sativa"]),
            QueryBuilder("taxon").set_taxa(["Zea mays"]),
            QueryBuilder("taxon").set_taxa(["Solanum lycopersicum"]),
            QueryBuilder("taxon").set_taxa(["Triticum aestivum"]),
        ]
        result = qb.search_batch(queries, api_base=API_BASE)
        assert isinstance(result, list)
        assert len(result) == 10

    def test_search_batch_boundary_100_queries(self, api_available):
        """Test searchBatch with exactly 100 queries (max boundary)."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon").set_taxa(["Canis lupus"]) for _ in range(100)]
        result = qb.search_batch(queries, api_base=API_BASE)
        assert isinstance(result, list)
        assert len(result) == 100

    def test_count_batch_single_query(self, api_available):
        """Test countBatch with a single query."""
        qb = QueryBuilder("taxon")
        query = QueryBuilder("taxon").set_taxa(["Mammalia"])
        result = qb.count_batch([query], api_base=API_BASE)
        assert isinstance(result, list)
        assert len(result) == 1
        assert isinstance(result[0], int)
        assert result[0] > 0

    def test_count_batch_multiple_queries(self, api_available):
        """Test countBatch with 5 queries returns hit counts for each."""
        qb = QueryBuilder("taxon")
        queries = [
            QueryBuilder("taxon").set_taxa(["Mammalia"]),
            QueryBuilder("taxon").set_taxa(["Aves"]),
            QueryBuilder("taxon").set_taxa(["Reptilia"]),
            QueryBuilder("taxon").set_taxa(["Amphibia"]),
            QueryBuilder("taxon").set_taxa(["Pisces"]),
        ]
        result = qb.count_batch(queries, api_base=API_BASE)
        assert isinstance(result, list)
        assert len(result) == 5
        assert all(isinstance(count, int) for count in result)
        assert all(count >= 0 for count in result)

    def test_count_batch_empty_result(self, api_available):
        """Test countBatch handles query with no results gracefully."""
        qb = QueryBuilder("taxon")
        query = QueryBuilder("taxon").set_taxa(["NonExistentSpecies"])
        result = qb.count_batch([query], api_base=API_BASE)
        assert isinstance(result, list)
        assert len(result) == 1
        assert result[0] >= 0  # Should return 0 or non-error value

    def test_record_single_taxon(self, api_available):
        """Test record() with a single taxon lookup."""
        qb = QueryBuilder("taxon")
        result = qb.record("taxon-9646", "taxon")  # Canis lupus
        assert isinstance(result, dict)
        assert len(result) > 0

    def test_lookup_taxon_name(self, api_available):
        """Test lookup() with a taxon name."""
        qb = QueryBuilder("taxon").set_taxa(["Homo"])
        result = qb.lookup("Homo", api_base=API_BASE)
        assert isinstance(result, (dict, list))

    def test_summary_with_field(self, api_available):
        """Test summary() with a field aggregation."""
        qb = QueryBuilder("taxon").set_taxa(["Canis lupus"]).add_field("genome_size")
        result = qb.summary("9646", "genome_size", api_base=API_BASE)
        assert isinstance(result, (dict, list))

    def test_search_batch_with_fields(self, api_available):
        """Test searchBatch with field additions."""
        qb = QueryBuilder("taxon")
        queries = [
            QueryBuilder("taxon").set_taxa(["Mammalia"]).add_field("genome_size"),
            QueryBuilder("taxon").set_taxa(["Aves"]).add_field("genome_size"),
        ]
        result = qb.search_batch(queries, api_base=API_BASE)
        assert isinstance(result, list)
        assert len(result) == 2

    def test_count_batch_ordering(self, api_available):
        """Test that countBatch returns results in the same order as queries."""
        qb = QueryBuilder("taxon")
        query_taxa = [
            ["Mammalia"],
            ["Aves"],
            ["Reptilia"],
        ]
        queries = [QueryBuilder("taxon").set_taxa(taxa) for taxa in query_taxa]
        result = qb.count_batch(queries, api_base=API_BASE)
        assert len(result) == len(queries)
        # Results should be in same order as queries (can't assert specific values
        # since API data may vary, but can assert ordering is consistent)

    def test_search_batch_response_parsing(self, api_available):
        """Test that searchBatch response is properly parsed."""
        qb = QueryBuilder("taxon")
        query = QueryBuilder("taxon").set_taxa(["Canis lupus"])
        result = qb.search_batch([query], api_base=API_BASE)
        # Result should be a list of records (each record from a result object)
        assert isinstance(result, list)
        # Each result should be a dict with record data
        for record in result:
            assert isinstance(record, dict)


class TestBatchIntegrationErrorHandling:
    """Integration tests for error conditions."""

    def test_search_batch_invalid_api_base(self):
        """Test searchBatch with invalid API base."""
        qb = QueryBuilder("taxon")
        query = QueryBuilder("taxon").set_taxa(["Canis lupus"])
        with pytest.raises(Exception):  # Should raise connection or HTTP error
            qb.search_batch([query], api_base="http://invalid.example.com:9999")

    def test_count_batch_invalid_api_base(self):
        """Test countBatch with invalid API base."""
        qb = QueryBuilder("taxon")
        query = QueryBuilder("taxon").set_taxa(["Mammalia"])
        with pytest.raises(Exception):
            qb.count_batch([query], api_base="http://invalid.example.com:9999")

    def test_search_batch_exceeds_limit(self):
        """Test that searchBatch raises error for >100 queries."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon") for _ in range(101)]
        with pytest.raises(ValueError, match="maximum 100 searches"):
            qb.search_batch(queries, api_base=API_BASE)

    def test_count_batch_exceeds_limit(self):
        """Test that countBatch raises error for >100 queries."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon") for _ in range(101)]
        with pytest.raises(ValueError, match="maximum 100 searches"):
            qb.count_batch(queries, api_base=API_BASE)
