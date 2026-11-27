# py-dependency-mapper

High-performance static analyzer to map Python dependencies — written in Rust and powered by the Ruff parser.

---

## Overview

`py-dependency-mapper` is a high-performance tool for analyzing static dependencies in Python projects.  
It is implemented in Rust and uses the **Ruff** parser to provide extremely fast and accurate parsing of import graphs.  

This makes it ideal for packaging (e.g., serverless deployments), dependency audits, or simply understanding the dependency graph of large applications.

---

## Features

⚡ **High performance** thanks to the Ruff parser.

🧩 **Two-phase architecture**: indexing and subgraph extraction per entry point.

🎯 **Prefix filtering** (e.g., `["my_app"]`) to reduce noise.

📦 **Complete pip package analysis** with automatic dependency resolution.

🛃 **Customizable manual mappings** via TOML files.

🔍 **Impact Analysis** (Reverse Lookups): Instantly find which files depend on a specific module (ideal for Smart Testing).

🧭 **Explain a dependency**: `explain_dependency` shows the chain of imports that links two files, or a file and a third-party library (debugging only).

📦 **Complete packages**: parent `__init__.py` files and the full `.dist-info` of each pip package are included.

🐍 **Python API** and CLI utilities.

🚀 **CI/CD friendly** — designed for large projects with hundreds or thousands of files.

---

## Installation

### From PyPI
```bash
pip install py-dependency-mapper
```

Requires Python **3.9 or newer**. The published wheels are built against the stable ABI (`abi3`),
so a single wheel per platform works on every supported Python version (including 3.14 and 3.15)
without recompiling.

Basic Usage

The workflow is designed to be efficient:

**Indexing Phase** — build a map of your entire project (or only the parts you're interested in).  
**Querying Phase** — use that map to instantly resolve the dependencies of specific entry points.    
**Package Analysis** — analyze and resolve dependencies of installed pip packages.

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
    project_module_prefixes=["my_app"],
    include_paths=["my_app/"],
    stdlib_list_path="/path/to/stdlib.txt"  # Optional
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

# Examine detailed information for each file
for path, file_info in dependency_graph.items():
    print(f"\nFile: {path}")
    print(f"  Hash: {file_info.hash}")
    print(f"  Stdlib imports: {file_info.stdlib_imports}")
    print(f"  Third party imports: {file_info.third_party_imports}")

# --- PHASE 3: Impact Analysis (Reverse Lookup) ---
# Ideal for CI/CD: Determine which tests to run based on changed files.
# If you modify 'utils.py', this tells you exactly which files import it (recursively).
# Accepts a LIST of changed files for batch processing.

changed_files = [
    "/path/to/project/my_app/utils.py",
    "/path/to/project/my_app/main.py"
]
print(f"\nCalculating impact for changes in: {changed_files}")

# Returns a Set of file paths that depend on ANY of the changed files
impacted_files = py_dependency_mapper.find_dependents(
    dependency_map=dependency_map,
    changed_file_paths=changed_files
)

print(f"This change affects {len(impacted_files)} files:")
for path in impacted_files:
    print(f"  -> {path}")

# Example CI Logic:
# tests_to_run = [f for f in impacted_files if f.startswith("tests/") or f.endswith("_test.py")]

```

#### Debugging: why does A depend on B?

`explain_dependency` is a debugging helper. It returns the shortest chain of imports that leads
from one file to another, so you can see *why* a test was selected or *why* a library ends up in a
package. Nothing else calls it, so it has no effect (and no cost) unless you use it.

```python
# Why is this test selected when utils.py changes?
chain = py_dependency_mapper.explain_dependency(
    dependency_map,
    source="/path/to/project/tests/test_main.py",
    target="/path/to/project/my_app/utils.py",
)
# ['/path/to/project/tests/test_main.py',
#  '/path/to/project/my_app/main.py',
#  '/path/to/project/my_app/utils.py']

# Why does this handler pull in a third-party library?
chain = py_dependency_mapper.explain_dependency(
    dependency_map, source="/path/to/project/my_app/handler.py", target="aiodynamo"
)
# The chain ends at the first file that imports the library.
```
---

## PIP Package Dependencies Analysis

The library includes advanced capabilities for analyzing installed pip packages:

### Dependency Tree Generation

```bash
# Generate dependency tree using uv
uv pip tree > dependencies.txt

# Then convert to JSON format using custom scripts
parse_pip_tree_to_dict(dependencies.txt)
```

### Custom conversion script
```python
def parse_pip_tree_to_dict(tree_output: str) -> dict:
    """
    TODO: https://github.com/astral-sh/uv/issues/4711
    Revisit when this issue is resolved. `uv` will then be able to
    generate JSON output directly, making this parser obsolete.
    """
    dependency_tree = {}
    stack = [(-1, dependency_tree)]

    for line in tree_output.strip().splitlines():
        indentation = len(line) - len(line.lstrip(" │├─└"))

        clean_line = line.lstrip(" │├─└").strip()

        is_duplicate = "(*)" in clean_line
        clean_line = clean_line.replace(" (*)", "").strip()

        match = re.match(r"([\w.-]+)(?:[=v\s]+)([\d.\w+-]+)", clean_line)
        if not match:
            continue

        package_name, version = match.groups()

        while stack[-1][0] >= indentation:
            stack.pop()

        parent_dependencies = stack[-1][1]

        current_package_node = {"version": version, "dependencies": {}}

        parent_dependencies[package_name] = current_package_node

        if not is_duplicate:
            stack.append((indentation, current_package_node["dependencies"]))

    return dependency_tree

```

### PIP Metadata Analysis


```python
# Build pip package metadata
pip_metadata = py_dependency_mapper.build_pip_metadata(
    dependency_tree_json_path="dependencies.json",
    site_packages_path="/path/to/site-packages",
    manual_mapping_path="mappings.toml"  # Optional: custom mappings
)

# Explore available information
print("Import to pip package mapping:")
pprint(pip_metadata.import_to_pip_map)

print("\nPackage information:")
for pkg_name, pkg_info in pip_metadata.pip_package_info_map.items():
    print(f"{pkg_name}: v{pkg_info.version}")
    print(f"  Dependencies: {pkg_info.dependencies}")
    print(f"  Installed paths: {pkg_info.installed_paths}")

```

`installed_paths` contains everything that must be copied to ship the package, relative to
`site_packages`: its top-level files and directories (e.g. `requests`, `six.py`), the console
scripts under `bin/` (e.g. `bin/tool`) and **the complete `.dist-info` directory**
(e.g. `requests-2.32.5.dist-info`). The `.dist-info` is always included, even if the package's
`RECORD` does not list it, because some libraries need it at runtime (`importlib.metadata`,
entry points, `pkg_resources`). It is not treated as an importable module, so it never appears in
`import_to_pip_map`.

### Package Set Resolution

```python
# Automatically resolve all dependencies for specific packages
resolved_packages = py_dependency_mapper.resolve_package_set(
    direct_packages=["requests", "numpy", "pandas"],
    pip_metadata=pip_metadata
)

print("Resolved packages with all their dependencies:")
for pkg_name, pkg_info in resolved_packages.items():
    print(f"  {pkg_name} v{pkg_info.version}")

```

🔧 Manual Mappings with TOML
For cases where automatic detection is not sufficient, you can use a TOML file for custom mappings:

```toml
# mappings.toml

# Map import names to pip package names
[import_mappings]
"cv2" = "opencv-python"
"sklearn" = "scikit-learn" 
"PIL" = "Pillow"
"yaml" = "PyYAML"

# Additional dependencies that should be included
[extra_dependencies]
"fastapi" = ["uvicorn", "python-multipart"]
"pydantic" = ["email-validator"]

# Trigger dependencies only when specific modules or attributes are imported in the code.
# Supports hierarchical matching: "openpyxl.drawing" matches "openpyxl.drawing.image.Image".
[conditional_dependencies]
"openpyxl.drawing" = ["pillow"]

# Additional package paths
[extra_package_paths]
"tensorflow" = ["bin", "include", "lib"]
"gremlinpython" = ["bin", "lib"]

```
## 📖 API Reference

```python
build_dependency_map(
    source_root: str,
    project_module_prefixes: List[str],
    include_paths: List[str],
    stdlib_list_path: Optional[str] = None
) -> Dict[str, ProjectFile]
```

Scans the project and builds the dependency map.

* **source_root**: Absolute path to the root of your source code.  

* **project_module_prefixes**: A list of module prefixes to include in the analysis (e.g., `["my_app"]`). A module matches when it is the prefix itself or one of its submodules (`my_app` matches `my_app` and `my_app.utils`, but not `my_app_extra` or `my_application`), so similarly named third-party packages are not mistaken for project code.  

* **include_paths**: A list of directories or files (relative to `source_root`) to begin the scan from.

* **stdlib_list_path**: Optional path to a file containing standard library module names.

* **returns**: A dictionary mapping file paths to `ProjectFile` objects.  

---

```python
get_dependency_graph(
    dependency_map: Dict,
    entry_point: str
) -> Dict[str, GraphFileResult]
```

From the pre-built map, gets the dependency subgraph for a specific entry point.

* **dependency_map**: The dictionary returned by `build_dependency_map`.  

* **entry_point**: The absolute path to the initial `.py` file.  

* **returns**: A dictionary mapping file paths to `GraphFileResult` objects.  

---

```python
find_dependents(
    dependency_map: Dict,
    changed_file_paths: List[str]
) -> Set[str]
```

Performs a reverse dependency lookup. Identifies all files in the project that depend on (import) any of the specified files, either directly or indirectly (transitively)

* **dependency_map**: The dictionary returned by `build_dependency_map`.
* **changed_file_paths**: A list of absolute paths to the files that were modified.
* **returns**: A Set of strings containing the absolute paths of all files that are impacted by the changes.

---

```python
explain_dependency(
    dependency_map: Dict,
    source: str,
    target: str
) -> Optional[List[str]]
```

Debugging helper. Finds the shortest chain of imports from `source` to `target`.

* **dependency_map**: The dictionary returned by `build_dependency_map`.
* **source**: Path of an indexed file, exactly as it appears as a key in the map. Raises `KeyError` if it is not indexed.
* **target**: Either the path of an indexed file, or the name of a third-party module (e.g. `"aiodynamo"`). For a third-party module the chain ends at the first file that imports it.
* **returns**: The list of file paths from `source` to the file that reaches `target`, or `None` if there is no path. When several chains have the same length the result is always the same one.

A test depends on a changed file exactly when `explain_dependency(map, test, changed)` returns a chain, so it answers "why was this test selected by `find_dependents`?". It is not meant for hot paths: it rebuilds an internal graph from the map on every call (about 0.1 s on 37,000 files).

---

### PIP Package Analysis Functions

```python
build_pip_metadata(
    dependency_tree_json_path: str,
    site_packages_path: str,
    manual_mapping_path: Optional[str] = None
) -> PipMetadata
```

Builds metadata for installed pip packages from a dependency tree JSON file.

* **dependency_tree_json_path**: Path to JSON file containing the dependency tree.

* **site_packages_path**: Path to the site-packages directory.

* **manual_mapping_path**: Optional path to TOML file with manual mappings.

* **returns**: A `PipMetadata` object containing package information and mappings.

```python
resolve_package_set(
    direct_packages: List[str],
    pip_metadata: PipMetadata
) -> Dict[str, PipPackageInfo]
```

Resolves all dependencies for a set of direct packages.

* **direct_packages**: List of package names to resolve dependencies for.

* **pip_metadata**: The `PipMetadata` object from `build_pip_metadata`.

* **returns**: A dictionary mapping package names to `PipPackageInfo` objects.

---


## Data Structures

### GraphFileResult

Contains information about a Python source file:

* `hash`: SHA256 hash of the file content.

* `project_imports`: List of project files this file depends on (file paths). Besides the modules it imports, it always includes the `__init__.py` of every package that contains the file (and of every package of each imported module), because Python runs them whenever the module is imported. A file with no project imports therefore still lists its parent `__init__.py` files, and so do the graphs returned by `get_dependency_graph`. Note that changing an `__init__.py` makes `find_dependents` return every file of its package.

* `stdlib_imports`: List of imported standard library modules.

* `third_party_imports`: List of imported third-party packages.



### PipMetadata
Contains pip package analysis results:

* `import_to_pip_map`: Mapping from import names to pip package names.

* `pip_package_info_map`: Mapping from pip package names to package information.

* `extra_dependencies_map`: Manual additional dependencies from TOML.

* `extra_paths_map`: Manual additional paths from TOML.

* `conditional_dependencies_map`: Manual conditional dependencies triggered by specific import prefixes.



### PipPackageInfo
Information about a specific pip package:

* `version`: Package version string.

* `installed_paths`: List of installed paths relative to `site_packages`: top-level files and
  directories, `bin/` scripts and the complete `.dist-info` directory. The order is not guaranteed.

* `dependencies`: List of direct dependency package names

---


## 📜 License

This project is licensed under the **MIT License**.  
See the [LICENSE](./LICENSE) file for more details.

---

## 🙌 Acknowledgements

This tool would not be possible without the incredible work of the team behind the **Ruff** project,  
whose high-performance parser is the heart of this analyzer.  

Ruff's license can be found in [`licenses/LICENSE-RUFF.md`](./licenses/LICENSE-RUFF.md).
