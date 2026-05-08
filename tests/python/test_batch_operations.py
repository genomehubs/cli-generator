"""Unit tests for batch SDK methods (search_batch, count_batch, record, lookup, summary).

Tests use mocked HTTP responses to validate:
1. Correct URL construction for batch endpoints
2. Correct request payload structure
3. Proper response parsing via parse_* functions
4. Constraint validation (max 100 searches per batch)
5. Error handling for HTTP failures and invalid inputs
"""

import json
from unittest.mock import MagicMock, mock_open, patch

import pytest

from cli_generator import QueryBuilder


class TestBatchConstraints:
    """Test constraint validation for batch operations."""

    def test_search_batch_enforces_max_100_queries(self):
        """search_batch should reject >100 queries."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon") for _ in range(101)]

        with pytest.raises(ValueError, match="maximum 100 searches per batch request"):
            qb.search_batch(queries)

    def test_count_batch_enforces_max_100_queries(self):
        """count_batch should reject >100 queries."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon") for _ in range(101)]

        with pytest.raises(ValueError, match="maximum 100 searches per batch request"):
            qb.count_batch(queries)

    def test_search_batch_accepts_1_query(self):
        """search_batch should accept 1 query."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon")]

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_resp = MagicMock()
            mock_resp.read.return_value = json.dumps({"status": {"success": True}, "results": [{"hits": 100}]}).encode(
                "utf-8"
            )
            mock_resp.__enter__.return_value = mock_resp
            mock_urlopen.return_value = mock_resp

            result = qb.search_batch(queries)
            assert isinstance(result, list)

    def test_search_batch_accepts_100_queries(self):
        """search_batch should accept exactly 100 queries."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon") for _ in range(100)]

        with patch("urllib.request.urlopen") as mock_urlopen:
            with patch("cli_generator.parse_batch_json") as mock_parse:
                mock_resp = MagicMock()
                results = [{"hits": i} for i in range(100)]
                response_data = {"status": {"success": True}, "results": results}
                mock_parse.return_value = json.dumps(response_data)
                mock_resp.read.return_value = json.dumps(response_data).encode("utf-8")
                mock_resp.__enter__.return_value = mock_resp
                mock_urlopen.return_value = mock_resp

                result = qb.search_batch(queries)
                assert len(result) == 100


class TestSearchBatchHTTPHandling:
    """Test HTTP request/response handling for search_batch."""

    def test_search_batch_constructs_correct_url(self):
        """search_batch should construct correct endpoint URL."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon").set_taxa(["Mammalia"])]

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_resp = MagicMock()
            mock_resp.read.return_value = json.dumps({"status": {"success": True}, "results": []}).encode("utf-8")
            mock_resp.__enter__.return_value = mock_resp
            mock_urlopen.return_value = mock_resp

            qb.search_batch(queries, api_base="http://localhost:3000/api")

            # Verify the URL was called
            call_args = mock_urlopen.call_args
            request_obj = call_args[0][0]
            assert "http://localhost:3000/api/v3/searchBatch" in request_obj.full_url

    def test_search_batch_uses_custom_api_version(self):
        """search_batch should respect custom api_version parameter."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon")]

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_resp = MagicMock()
            mock_resp.read.return_value = json.dumps({"status": {"success": True}, "results": []}).encode("utf-8")
            mock_resp.__enter__.return_value = mock_resp
            mock_urlopen.return_value = mock_resp

            qb.search_batch(queries, api_base="http://localhost:3000/api", api_version="v4")

            call_args = mock_urlopen.call_args
            request_obj = call_args[0][0]
            assert "v4/searchBatch" in request_obj.full_url

    def test_search_batch_request_has_json_content_type(self):
        """search_batch should set Content-Type to application/json."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon")]

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_resp = MagicMock()
            mock_resp.read.return_value = json.dumps({"status": {"success": True}, "results": []}).encode("utf-8")
            mock_resp.__enter__.return_value = mock_resp
            mock_urlopen.return_value = mock_resp

            qb.search_batch(queries)

            call_args = mock_urlopen.call_args
            request_obj = call_args[0][0]
            assert request_obj.headers.get("Content-type") == "application/json"

    def test_search_batch_payload_structure(self):
        """search_batch should send payload with 'searches' array."""
        qb = QueryBuilder("taxon")
        queries = [
            QueryBuilder("taxon").set_taxa(["Mammalia"]),
            QueryBuilder("taxon").set_taxa(["Aves"]),
        ]

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_resp = MagicMock()
            mock_resp.read.return_value = json.dumps({"status": {"success": True}, "results": [{}, {}]}).encode("utf-8")
            mock_resp.__enter__.return_value = mock_resp
            mock_urlopen.return_value = mock_resp

            qb.search_batch(queries)

            call_args = mock_urlopen.call_args
            request_obj = call_args[0][0]
            payload = json.loads(request_obj.data.decode("utf-8"))

            assert "searches" in payload
            assert len(payload["searches"]) == 2
            assert all("query_yaml" in s and "params_yaml" in s for s in payload["searches"])


class TestCountBatchHTTPHandling:
    """Test HTTP request/response handling for count_batch."""

    def test_count_batch_constructs_correct_url(self):
        """count_batch should construct correct endpoint URL."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon").set_taxa(["Mammalia"])]

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_resp = MagicMock()
            mock_resp.read.return_value = json.dumps(
                {"status": {"success": True, "hits": 100}, "results": [{"hits": 100}]}
            ).encode("utf-8")
            mock_resp.__enter__.return_value = mock_resp
            mock_urlopen.return_value = mock_resp

            qb.count_batch(queries)

            call_args = mock_urlopen.call_args
            request_obj = call_args[0][0]
            assert "v3/countBatch" in request_obj.full_url

    def test_count_batch_returns_hit_counts(self):
        """count_batch should return list of hit counts."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon"), QueryBuilder("taxon")]

        with patch("urllib.request.urlopen") as mock_urlopen:
            with patch("cli_generator.parse_batch_json") as mock_parse:
                mock_resp = MagicMock()
                response_data = {
                    "status": {"success": True},
                    "results": [
                        {"status": {"hits": 1000}},
                        {"status": {"hits": 2000}},
                    ],
                }
                mock_parse.return_value = json.dumps(response_data)
                mock_resp.read.return_value = json.dumps(response_data).encode("utf-8")
                mock_resp.__enter__.return_value = mock_resp
                mock_urlopen.return_value = mock_resp

                result = qb.count_batch(queries)

                assert result == [1000, 2000]


class TestRecordHTTPHandling:
    """Test HTTP request/response handling for record."""

    def test_record_constructs_correct_url(self):
        """record should construct correct endpoint URL."""
        qb = QueryBuilder("taxon").set_taxa(["9646"])

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_resp = MagicMock()
            mock_resp.read.return_value = json.dumps({"status": {"success": True}, "results": []}).encode("utf-8")
            mock_resp.__enter__.return_value = mock_resp
            mock_urlopen.return_value = mock_resp

            qb.record("9646", api_base="http://localhost:3000/api")

            call_args = mock_urlopen.call_args
            url_called = call_args[0][0]
            assert "v3/record" in url_called

    def test_record_uses_get_method(self):
        """record should use GET method with query params."""
        qb = QueryBuilder("taxon").set_taxa(["9646"])

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_resp = MagicMock()
            mock_resp.read.return_value = json.dumps({"status": {"success": True}, "results": []}).encode("utf-8")
            mock_resp.__enter__.return_value = mock_resp
            mock_urlopen.return_value = mock_resp

            qb.record("9646")

            call_args = mock_urlopen.call_args
            url_called = call_args[0][0]
            assert "recordId=9646" in url_called


class TestLookupHTTPHandling:
    """Test HTTP request/response handling for lookup."""

    def test_lookup_constructs_correct_url(self):
        """lookup should construct correct endpoint URL."""
        qb = QueryBuilder("taxon").set_taxa(["9646"])

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_resp = MagicMock()
            mock_resp.read.return_value = json.dumps({"status": {"success": True}, "results": []}).encode("utf-8")
            mock_resp.__enter__.return_value = mock_resp
            mock_urlopen.return_value = mock_resp

            qb.lookup("9646", api_base="http://localhost:3000/api")

            call_args = mock_urlopen.call_args
            url_called = call_args[0][0]
            assert "v3/lookup" in url_called


class TestSummaryHTTPHandling:
    """Test HTTP request/response handling for summary."""

    def test_summary_constructs_correct_url(self):
        """summary should construct correct endpoint URL."""
        qb = QueryBuilder("taxon").add_field("genome_size")

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_resp = MagicMock()
            mock_resp.read.return_value = json.dumps({"status": {"success": True}, "results": []}).encode("utf-8")
            mock_resp.__enter__.return_value = mock_resp
            mock_urlopen.return_value = mock_resp

            qb.summary("9646", "genome_size", api_base="http://localhost:3000/api")

            call_args = mock_urlopen.call_args
            url_called = call_args[0][0]
            assert "v3/summary" in url_called


class TestErrorHandling:
    """Test error handling in batch methods."""

    def test_search_batch_handles_http_error(self):
        """search_batch should propagate HTTP errors."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon")]

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_urlopen.side_effect = Exception("HTTP 500: Server Error")

            with pytest.raises(Exception, match="HTTP 500"):
                qb.search_batch(queries)

    def test_count_batch_handles_http_error(self):
        """count_batch should propagate HTTP errors."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon")]

        with patch("urllib.request.urlopen") as mock_urlopen:
            mock_urlopen.side_effect = Exception("HTTP 500: Server Error")

            with pytest.raises(Exception, match="HTTP 500"):
                qb.count_batch(queries)

    def test_search_batch_handles_malformed_response(self):
        """search_batch should handle malformed JSON responses gracefully."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon")]

        with patch("urllib.request.urlopen") as mock_urlopen:
            with patch("cli_generator.parse_batch_json") as mock_parse:
                mock_resp = MagicMock()
                mock_resp.read.return_value = b"invalid json"
                mock_resp.__enter__.return_value = mock_resp
                mock_urlopen.return_value = mock_resp
                # parse_batch_json can transform the response; if it returns invalid JSON, json.loads will fail
                mock_parse.return_value = "not valid json"

                # This should raise an error since json.loads will fail
                with pytest.raises(json.JSONDecodeError):
                    qb.search_batch(queries)


class TestResponseParsing:
    """Test response parsing integration."""

    def test_search_batch_returns_results_array(self):
        """search_batch should return array of result objects."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon"), QueryBuilder("taxon")]

        with patch("urllib.request.urlopen") as mock_urlopen:
            with patch("cli_generator.parse_batch_json") as mock_parse:
                mock_resp = MagicMock()
                response_data = {
                    "status": {"success": True},
                    "results": [{"hits": 100, "result": "obj1"}, {"hits": 50, "result": "obj2"}],
                }
                mock_parse.return_value = json.dumps(response_data)
                mock_resp.read.return_value = json.dumps(response_data).encode("utf-8")
                mock_resp.__enter__.return_value = mock_resp
                mock_urlopen.return_value = mock_resp

                result = qb.search_batch(queries)

                assert isinstance(result, list)
                assert len(result) == 2
                assert result[0]["hits"] == 100
                assert result[1]["hits"] == 50

    def test_count_batch_extracts_hits_from_each_result(self):
        """count_batch should extract status.hits from each result."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon"), QueryBuilder("taxon"), QueryBuilder("taxon")]

        with patch("urllib.request.urlopen") as mock_urlopen:
            with patch("cli_generator.parse_batch_json") as mock_parse:
                mock_resp = MagicMock()
                response_data = {
                    "status": {"success": True},
                    "results": [
                        {"status": {"hits": 150}},
                        {"status": {"hits": 250}},
                        {"status": {"hits": 350}},
                    ],
                }
                mock_parse.return_value = json.dumps(response_data)
                mock_resp.read.return_value = json.dumps(response_data).encode("utf-8")
                mock_resp.__enter__.return_value = mock_resp
                mock_urlopen.return_value = mock_resp

                result = qb.count_batch(queries)

                assert result == [150, 250, 350]

    def test_search_batch_handles_empty_results(self):
        """search_batch should handle responses with no results."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon")]

        with patch("urllib.request.urlopen") as mock_urlopen:
            with patch("cli_generator.parse_batch_json") as mock_parse:
                mock_resp = MagicMock()
                response_data = {"status": {"success": True}, "results": []}
                mock_parse.return_value = json.dumps(response_data)
                mock_resp.read.return_value = json.dumps(response_data).encode("utf-8")
                mock_resp.__enter__.return_value = mock_resp
                mock_urlopen.return_value = mock_resp

                result = qb.search_batch(queries)

                assert result == []

    def test_count_batch_handles_empty_results(self):
        """count_batch should handle responses with no results."""
        qb = QueryBuilder("taxon")
        queries = [QueryBuilder("taxon")]

        with patch("urllib.request.urlopen") as mock_urlopen:
            with patch("cli_generator.parse_batch_json") as mock_parse:
                mock_resp = MagicMock()
                response_data = {"status": {"success": True}, "results": []}
                mock_parse.return_value = json.dumps(response_data)
                mock_resp.read.return_value = json.dumps(response_data).encode("utf-8")
                mock_resp.__enter__.return_value = mock_resp
                mock_urlopen.return_value = mock_resp

                result = qb.count_batch(queries)

                assert result == []
