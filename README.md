# py-dependency-mapper

High-performance static analyzer to map Python dependencies — written in Rust and powered by the Ruff parser.

---

## Overview

`py-dependency-mapper` is a high-performance tool for analyzing static dependencies in Python projects.  
It is implemented in Rust and uses the **Ruff** parser to provide extremely fast and accurate parsing of import graphs.  

This makes it ideal for packaging (e.g., serverless deployments), dependency audits, or simply understanding the dependency graph of large applications.

---

## Features

:zap: **High performance** thanks to the Ruff parser.

:jigsaw: **Two-phase architecture**: indexing and subgraph extraction per entry point.

:dart: **Prefix filtering** (e.g., `["my_app"]`) to reduce noise.

:snake: **Python API** and CLI utilities.

:rocket: **CI/CD friendly** — designed for large projects with hundreds or thousands of files.

---

## Installation

### From PyPI
```bash
pip install py-dependency-mapper
```

Basic Usage

The workflow is designed to be efficient:

**Indexing Phase** — build a map of your entire project (or only the parts you're interested in).  
**Querying Phase** — use that map to instantly resolve the dependencies of specific entry points.

---

### Example Project
```bash
/path/to/project/
└── my_app/
    ├── __init__.py
    ├── main.py       # imports utils
    └── utils.py      # has no other local imports
```

### Usage

```python
import py_dependency_mapper
from pprint import pprint

# --- PHASE 1: Indexing (Done once at the start) ---
# This builds a complete map of all files, their hashes, and their imports.
# This is the heavy operation, but it's only done once.
print("Building the project's dependency map...")

dependency_map = py_dependency_mapper.build_dependency_map(
    source_root="/path/to/project",
    include_paths=["my_app/"],
    filter_prefixes=["my_app"]
)
print(f"Map built with {len(dependency_map)} files.")
# Expected output: Map built with 3 files.

# --- PHASE 2: Querying (Done as many times as you need) ---
# Now, for any Lambda or application entry point, you can get
# its specific dependency graph almost instantly.
entry_point = "/path/to/project/my_app/main.py"

print(f"\nGetting dependency graph for: {entry_point}")
# This call is extremely fast because it only queries the in-memory map.
dependency_graph = py_dependency_mapper.get_dependency_graph(
    dependency_map=dependency_map,
    entry_point=entry_point
)

print(f"The entry point requires {len(dependency_graph)} total files.")
# Expected output: The entry point requires 3 total files.

# `dependency_graph` is now a dictionary of {file_path: hash}
# ready to be used for building an asset hash or a ZIP file.
pprint(dependency_graph)
```

**Expected Output**
```
Building the project's dependency map...
Map built with 3 files.

Getting dependency graph for: /path/to/project/my_app/main.py
The entry point requires 3 total files.
{
  '/path/to/project/my_app/__init__.py': 'e3b0c442...',
  '/path/to/project/my_app/main.py': '...',
  '/path/to/project/my_app/utils.py': '...'
}
```
---

## :book: API Reference

```python
build_dependency_map(
    source_root: str,
    include_paths: List[str],
    filter_prefixes: List[str]
) -> Dict[str, ProjectFile]
```

Scans the project and builds the dependency map.

**source_root**: Absolute path to the root of your source code.  

**include_paths**: A list of directories or files (relative to `source_root`) to begin the scan from.  

**filter_prefixes**: A list of module prefixes to include in the analysis (e.g., `["my_app"]`).  

**returns**: A dictionary mapping file paths to `ProjectFile` objects.  

---

```python
pythonget_dependency_graph(
    dependency_map: Dict,
    entry_point: str
) -> Dict[str, str]
```

From the pre-built map, gets the dependency subgraph for a specific entry point.

**dependency_map**: The dictionary returned by `build_dependency_map`.  

**entry_point**: The absolute path to the initial `.py` file.  

**returns**: A dictionary mapping `{file_path: hash}` for all dependencies required by the entry point.  

---

## :scroll: License

This project is licensed under the **MIT License**.  
See the [LICENSE](./LICENSE) file for more details.

---

## :raised_hands: Acknowledgements

This tool would not be possible without the incredible work of the team behind the **Ruff** project,  
whose high-performance parser is the heart of this analyzer.  

Ruff's license can be found in [`licenses/LICENSE-RUFF.md`](./licenses/LICENSE-RUFF.md).
