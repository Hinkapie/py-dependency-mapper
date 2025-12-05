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

🐍 **Python API** and CLI utilities.

🚀 **CI/CD friendly** — designed for large projects with hundreds or thousands of files.

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

* **project_module_prefixes**: A list of module prefixes to include in the analysis (e.g., `["my_app"]`).  

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

* `project_imports`: List of imported project modules (file paths).

* `stdlib_imports`: List of imported standard library modules.

* `third_party_imports`: List of imported third-party packages.



### PipMetadata
Contains pip package analysis results:

* `import_to_pip_map`: Mapping from import names to pip package names.

* `pip_package_info_map`: Mapping from pip package names to package information.

* `extra_dependencies_map`: Manual additional dependencies from TOML.

* `extra_paths_map`: Manual additional paths from TOML.



### PipPackageInfo
Information about a specific pip package:

* `version`: Package version string.

* `installed_paths`: List of installed file/directory paths.

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
