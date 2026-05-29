import pytest

from cli_generator import QueryBuilder, ReportBuilder


def test_report_batch_exceeds_limit():
    qb = QueryBuilder("taxon")
    rb = ReportBuilder("histogram")
    reports = [rb] * 101
    with pytest.raises(ValueError):
        qb.report_batch(reports)
