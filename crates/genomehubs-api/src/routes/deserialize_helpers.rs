// This module contains helper utilities for deserialization patterns.
// Custom deserializers are currently implemented directly in route structs (search, countBatch, searchBatch)
// to support flexible formats: both "query_yaml"/"query" and "params_yaml"/"params" field names,
// with automatic conversion of JSON objects to YAML strings.
