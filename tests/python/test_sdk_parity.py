"""Test SDK parity across Python, JavaScript, and R.

This module verifies that all three generated SDKs (Python, JavaScript, R)
maintain consistent method signatures and configuration parameters.
"""

import re
from pathlib import Path

import pytest

PROJECT_ROOT = Path(__file__).parent.parent.parent

# ── Canonical method definitions ──────────────────────────────────────────────

CANONICAL_METHODS = {
    "set_taxa": {
        "params": ["taxa", "filter_type"],
        "python_name": "set_taxa",
        "js_name": "setTaxa",
        "r_name": "set_taxa",
    },
    "set_rank": {
        "params": ["rank"],
        "python_name": "set_rank",
        "js_name": "setRank",
        "r_name": "set_rank",
    },
    "set_assemblies": {
        "params": ["assemblies"],
        "python_name": "set_assemblies",
        "js_name": "setAssemblies",
        "r_name": "set_assemblies",
    },
    "set_samples": {
        "params": ["samples"],
        "python_name": "set_samples",
        "js_name": "setSamples",
        "r_name": "set_samples",
    },
    "add_attribute": {
        "params": ["name", "operator", "value", "modifiers"],
        "python_name": "add_attribute",
        "js_name": "addAttribute",
        "r_name": "add_attribute",
    },
    "set_attributes": {
        "params": ["attributes"],
        "python_name": "set_attributes",
        "js_name": "setAttributes",
        "r_name": "set_attributes",
    },
    "add_field": {
        "params": ["name", "modifiers"],
        "python_name": "add_field",
        "js_name": "addField",
        "r_name": "add_field",
    },
    "set_fields": {
        "params": ["fields"],
        "python_name": "set_fields",
        "js_name": "setFields",
        "r_name": "set_fields",
    },
    "set_names": {
        "params": ["name_classes"],
        "python_name": "set_names",
        "js_name": "setNames",
        "r_name": "set_names",
    },
    "set_ranks": {
        "params": ["ranks"],
        "python_name": "set_ranks",
        "js_name": "setRanks",
        "r_name": "set_ranks",
    },
    "set_exclude_ancestral": {
        "params": ["fields"],
        "python_name": "set_exclude_ancestral",
        "js_name": "setExcludeAncestral",
        "r_name": "set_exclude_ancestral",
    },
    "add_exclude_ancestral": {
        "params": ["field"],
        "python_name": "add_exclude_ancestral",
        "js_name": "addExcludeAncestral",
        "r_name": "add_exclude_ancestral",
    },
    "set_exclude_descendant": {
        "params": ["fields"],
        "python_name": "set_exclude_descendant",
        "js_name": "setExcludeDescendant",
        "r_name": "set_exclude_descendant",
    },
    "add_exclude_descendant": {
        "params": ["field"],
        "python_name": "add_exclude_descendant",
        "js_name": "addExcludeDescendant",
        "r_name": "add_exclude_descendant",
    },
    "set_exclude_direct": {
        "params": ["fields"],
        "python_name": "set_exclude_direct",
        "js_name": "setExcludeDirect",
        "r_name": "set_exclude_direct",
    },
    "add_exclude_direct": {
        "params": ["field"],
        "python_name": "add_exclude_direct",
        "js_name": "addExcludeDirect",
        "r_name": "add_exclude_direct",
    },
    "set_exclude_missing": {
        "params": ["fields"],
        "python_name": "set_exclude_missing",
        "js_name": "setExcludeMissing",
        "r_name": "set_exclude_missing",
    },
    "add_exclude_missing": {
        "params": ["field"],
        "python_name": "add_exclude_missing",
        "js_name": "addExcludeMissing",
        "r_name": "add_exclude_missing",
    },
    "set_exclude_derived": {
        "params": ["fields"],
        "python_name": "set_exclude_derived",
        "js_name": "setExcludeDerived",
        "r_name": "set_exclude_derived",
    },
    "set_exclude_estimated": {
        "params": ["fields"],
        "python_name": "set_exclude_estimated",
        "js_name": "setExcludeEstimated",
        "r_name": "set_exclude_estimated",
    },
    "set_size": {
        "params": ["size"],
        "python_name": "set_size",
        "js_name": "setSize",
        "r_name": "set_size",
    },
    "set_page": {
        "params": ["page"],
        "python_name": "set_page",
        "js_name": "setPage",
        "r_name": "set_page",
    },
    "set_sort": {
        "params": ["sort_by", "direction"],
        "python_name": "set_sort",
        "js_name": "setSort",
        "r_name": "set_sort",
    },
    "set_include_estimates": {
        "params": ["value"],
        "python_name": "set_include_estimates",
        "js_name": "setIncludeEstimates",
        "r_name": "set_include_estimates",
    },
    "set_taxonomy": {
        "params": ["taxonomy"],
        "python_name": "set_taxonomy",
        "js_name": "setTaxonomy",
        "r_name": "set_taxonomy",
    },
    "to_query_yaml": {
        "params": [],
        "python_name": "to_query_yaml",
        "js_name": "toQueryYaml",
        "r_name": "to_query_yaml",
    },
    "to_params_yaml": {
        "params": [],
        "python_name": "to_params_yaml",
        "js_name": "toParamsYaml",
        "r_name": "to_params_yaml",
    },
    "to_url": {
        "params": [],
        "python_name": "to_url",
        "js_name": "toUrl",
        "r_name": "to_url",
    },
    "to_ui_url": {
        "params": [],
        "python_name": "to_ui_url",
        "js_name": "toUiUrl",
        "r_name": "to_ui_url",
    },
    "count": {
        "params": [],
        "python_name": "count",
        "js_name": "count",
        "r_name": "count",
    },
    "search": {
        "params": ["format"],
        "python_name": "search",
        "js_name": "search",
        "r_name": "search",
    },
    "validate": {
        "params": [],
        "python_name": "validate",
        "js_name": "validate",
        "r_name": "validate",
    },
    "describe": {
        "params": ["field_metadata", "mode"],
        "python_name": "describe",
        "js_name": "describe",
        "r_name": "describe",
    },
    "snippet": {
        "params": ["languages", "site_name", "sdk_name", "api_base"],
        "python_name": "snippet",
        "js_name": "snippet",
        "r_name": "snippet",
    },
    "reset": {
        "params": [],
        "python_name": "reset",
        "js_name": "reset",
        "r_name": "reset",
    },
    "merge": {
        "params": ["other"],
        "python_name": "merge",
        "js_name": "merge",
        "r_name": "merge",
    },
}

CONSTRUCTOR_PARAMS: dict[str, dict[str, str]] = {}

# ── Introspection functions ──────────────────────────────────────────────────


def get_python_constructor_params():
    """Extract constructor parameters from Python template."""
    query_py_tera = PROJECT_ROOT / "templates" / "python" / "query.py.tera"
    content = Path(query_py_tera).read_text()
    # Find __init__ signature and extract parameter names
    pattern = r"def __init__\s*\(\s*self,([^)]+)\)"
    match = re.search(pattern, content, re.DOTALL)
    if not match:
        return []

    params_str = match[1]
    params = []
    for p in params_str.split(","):
        if p := p.strip():
            param_name = p.split(":")[0].strip()
            params.append(param_name)
    return params


def get_js_constructor_params():
    """Extract constructor parameters from JavaScript template."""
    query_js = PROJECT_ROOT / "templates" / "js" / "query.js"
    content = Path(query_js).read_text()
    # constructor(index, options = {}) pattern
    pattern = r"constructor\s*\(([^)]+)\)"
    match = re.search(pattern, content, re.DOTALL)
    if not match:
        return []

    params_str = match[1]
    params = []
    for p in params_str.split(","):
        if p := p.strip():
            param_name = p.split("=")[0].strip()
            params.append(param_name)
    return params


def get_r_constructor_params():
    """Extract constructor parameters from R template."""
    query_r = PROJECT_ROOT / "templates" / "r" / "query.R"
    content = Path(query_r).read_text()
    # Find initialize = function(...) pattern
    pattern = r"initialize\s*=\s*function\s*\(([^)]+)\)"
    match = re.search(pattern, content, re.DOTALL)
    if not match:
        return []

    params_str = match[1]
    params = []
    for p in params_str.split(","):
        p = p.strip()
        if p and p != "self":
            param_name = p.split("=")[0].strip()
            params.append(param_name)
    return params


def get_python_methods():
    """Extract all public methods from templates/python/query.py.tera (generated SDK)."""
    # Use the template, not the main SDK
    query_py_tera = PROJECT_ROOT / "templates" / "python" / "query.py.tera"
    assert query_py_tera.exists(), f"Python query template not found at {query_py_tera}"

    content = Path(query_py_tera).read_text()
    # Rough parsing: look for "def method_name("
    methods = {}
    pattern = r"^\s{4}def\s+(\w+)\s*\("
    for match in re.finditer(pattern, content, re.MULTILINE):
        name = match.group(1)
        if not name.startswith("_"):
            # Find the full method signature (may span multiple lines)
            # Look from the opening paren to the closing paren
            paren_start = match.end() - 1  # Position of the '('
            paren_depth = 0
            paren_end = paren_start

            for i in range(paren_start, len(content)):
                if content[i] == "(":
                    paren_depth += 1
                elif content[i] == ")":
                    paren_depth -= 1
                    if paren_depth == 0:
                        paren_end = i
                        break

            # Extract parameters from the signature
            params_str = content[paren_start + 1 : paren_end]
            params = [p.strip() for p in params_str.split(",") if p.strip() and p.strip() != "self"]
            # Remove type annotations and defaults
            params = [p.split(":")[0].split("=")[0].strip() for p in params]
            methods[name] = params

    return methods


def get_js_methods():
    """Extract all public methods from templates/js/query.js."""
    query_js = PROJECT_ROOT / "templates" / "js" / "query.js"
    assert query_js.exists(), f"JavaScript query template not found at {query_js}"

    content = Path(query_js).read_text()
    methods = {}
    # Look for method definitions: methodName(params) {
    pattern = r"(\w+)\s*\(\s*([^)]*)\s*\)\s*{"
    for match in re.finditer(pattern, content):
        name = match.group(1)
        if not name.startswith("_") and name not in ("constructor", "if", "for", "while"):
            params_str = match.group(2)
            params = [p.strip() for p in params_str.split(",") if p.strip() and p.strip() != "this"]
            # Remove default values and destructuring
            params = [p.split("=")[0].split("{")[0].split("}")[0].strip() for p in params]
            methods[name] = [p for p in params if p]

    return methods


def get_r_methods():
    """Extract all public methods from templates/r/query.R."""
    query_r = PROJECT_ROOT / "templates" / "r" / "query.R"
    assert query_r.exists(), f"R query template not found at {query_r}"

    content = Path(query_r).read_text()
    methods = {}
    # Look for method definitions: method_name = function(...) {
    pattern = r"(\w+)\s*=\s*function\s*\(([^)]*)\)"
    for match in re.finditer(pattern, content):
        name = match.group(1)
        if not name.startswith("_") and name != "private":
            params_str = match.group(2)
            params = [p.strip() for p in params_str.split(",") if p.strip()]
            params = [p.split("=")[0].strip() for p in params]
            methods[name] = [p for p in params if p]

    return methods


def get_python_docstring(method_name: str) -> str:
    """Get the docstring for a Python template method.

    For __init__, returns the class docstring (standard Python convention).
    """
    query_py_tera = PROJECT_ROOT / "templates" / "python" / "query.py.tera"
    content = Path(query_py_tera).read_text()
    if method_name == "__init__":
        # For __init__, return the class docstring (Python convention)
        class_pattern = r"class\s+QueryBuilder.*?:\s*\n\s+\"\"\"(.*?)\"\"\""
        match = re.search(class_pattern, content, re.DOTALL)
    else:
        # For other methods, search for "def method_name" and its docstring
        method_pattern = rf'def\s+{method_name}\s*\([^)]*\).*?:\s*\n\s+"""(.*?)"""'
        match = re.search(method_pattern, content, re.DOTALL)

    return match[1].strip() if match else ""


# ── Tests ────────────────────────────────────────────────────────────────────


class TestSDKParity:
    """Test that all three SDKs have consistent method signatures."""

    def test_python_canonical_methods_present(self):
        """All canonical methods must exist in Python SDK."""
        python_methods = get_python_methods()

        for concept, spec in CANONICAL_METHODS.items():
            method_name = spec["python_name"]
            assert method_name in python_methods, f"Python missing method: {method_name}"

    def test_javascript_canonical_methods_present(self):
        """All canonical methods must exist in JavaScript SDK."""
        js_methods = get_js_methods()

        for concept, spec in CANONICAL_METHODS.items():
            method_name = spec["js_name"]
            assert method_name in js_methods, f"JavaScript missing method: {method_name}"

    def test_r_canonical_methods_present(self):
        """All canonical methods must exist in R SDK."""
        r_methods = get_r_methods()

        for concept, spec in CANONICAL_METHODS.items():
            method_name = spec["r_name"]
            assert method_name in r_methods, f"R missing method: {method_name}"

    def test_no_extra_methods_in_python(self):
        """Python should not have extra methods beyond canonical set."""
        python_methods = get_python_methods()
        canonical_python_names = {spec["python_name"] for spec in CANONICAL_METHODS.values()}
        # Allow documented utility methods
        canonical_python_names.update(
            [
                "__init__",
                "field_modifiers",
                "to_tidy_records",
                "field_names",
                "field_info",
                "combine",
                "search_df",
                "search_polars",
                "search_all",
            ]
        )

        extra = set(python_methods.keys()) - canonical_python_names
        assert len(extra) == 0, f"Python has extra methods not in canonical list: {extra}"


class TestValidationConfiguration:
    """Test that validation is properly implemented in templates."""

    def test_python_validate_method_exists(self):
        """Python template should have validate() method."""
        python_methods = get_python_methods()
        assert "validate" in python_methods, "Python template missing validate() method"

    def test_r_validate_method_exists(self):
        """R template should have validate() method."""
        r_methods = get_r_methods()
        assert "validate" in r_methods, "R template missing validate() method"


class TestDocumentationParity:
    """Test that Quarto reference documentation includes all SDK methods."""

    def get_documented_methods(self) -> set[str]:
        """Return the set of canonical method names that appear in the Quarto reference.

        Uses a simple membership check: a method is considered documented if its
        name appears in a backtick context anywhere in the file.  This is robust
        against heading style, table vs list format, and any punctuation convention
        — the only requirement is that the method name is mentioned at least once
        as a backtick-quoted identifier.
        """
        quarto_path = PROJECT_ROOT / "workdir/my-goat/goat-cli/docs/reference/query-builder.qmd"
        if not quarto_path.exists():
            pytest.skip(
                f"Quarto reference guide not found at {quarto_path}. "
                "This test requires the generated goat CLI project."
            )

        content = quarto_path.read_text()
        canonical_names = set(CANONICAL_METHODS.keys())
        return {name for name in canonical_names if f"`{name}" in content}

    def test_documented_methods_include_all_canonical(self):
        """All canonical methods should be documented in Quarto reference."""
        documented = self.get_documented_methods()
        canonical_names = set(CANONICAL_METHODS.keys())

        missing = canonical_names - documented
        assert len(missing) == 0, f"Documentation missing these canonical methods: {sorted(missing)}"

    def test_documented_methods_include_utilities(self):
        """Documentation should include documented utility methods."""
        quarto_path = PROJECT_ROOT / "workdir/my-goat/goat-cli/docs/reference/query-builder.qmd"
        if not quarto_path.exists():
            pytest.skip(
                f"Quarto reference guide not found at {quarto_path}. "
                "This test requires the generated goat CLI project."
            )

        content = quarto_path.read_text()

        # These utility methods are not in CANONICAL_METHODS (they are Python-only wrappers)
        # but should still appear in the reference documentation.
        utilities = {
            "search_df",  # pandas wrapper
            "search_polars",  # polars wrapper
            "search_all",  # pagination wrapper
        }

        for util in utilities:
            assert f"`{util}" in content, f"Documentation missing utility method: {util}"

    def test_documented_methods_reference_parameters(self):
        """Documented methods should include parameter tables where applicable."""
        quarto_path = PROJECT_ROOT / "workdir/my-goat/goat-cli/docs/reference/query-builder.qmd"
        if not quarto_path.exists():
            pytest.skip(
                f"Quarto reference guide not found at {quarto_path}. "
                "This test requires the generated goat CLI project."
            )

        content = quarto_path.read_text()

        # Check that key methods with parameters have tables
        methods_with_params = {
            "set_taxa": ["taxa", "filter_type"],
            "add_attribute": ["name", "operator", "value", "modifiers"],
        }

        for method, expected_params in methods_with_params.items():
            # Find the method section
            method_pattern = rf"^###\s+`{method}\("
            assert re.search(method_pattern, content, re.MULTILINE), f"Method {method} not found in documentation"

            # Check for parameter table after the method heading
            for param in expected_params:
                param_pattern = rf"(?:{method}.*?){param}"
                assert re.search(
                    param_pattern, content, re.DOTALL
                ), f"Parameter {param} for method {method} not documented"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
