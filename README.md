py-dependency-mapper
A high-performance tool to analyze Python project dependencies, written in Rust and powered by the Ruff parser.

🤔 Why py-dependency-mapper?
Packaging Python applications, especially for serverless environments like AWS Lambda, requires knowing precisely which source files must be included. Existing tools can be slow when analyzing large codebases, as they often rely on Python's runtime introspection.

py-dependency-mapper solves this by using a very fast static analysis engine written in Rust. Instead of executing the code, it reads it and builds a complete map of all interconnections (imports), allowing you to package your applications quickly and accurately.

✨ Key Features
High Performance: Being written in native Rust, it is significantly faster than pure Python alternatives, especially on projects with hundreds or thousands of files.

Accurate Analysis: It uses the same parser as Ruff, the world's fastest Python linter, to ensure robust and precise code analysis.

Two-Phase Architecture:

Indexing: Scans your project once to create an in-memory dependency map (build_dependency_map).

Querying: Performs near-instantaneous queries on that map to get the dependency tree for any given entry point (get_dependency_graph).

Ideal for CI/CD: Its speed and architecture make it a perfect fit for continuous integration and deployment pipelines, where build time is critical.

🚀 Installation
Bash

pip install py-dependency-mapper
💻 Basic Usage
The workflow is designed to be efficient. First, you build a map of your entire project (or the parts you're interested in). Then, you use that map to resolve the dependencies for your specific entry points.

Let's imagine a simple project structure:

/path/to/project/
└── my_app/
    ├── __init__.py
    ├── main.py       # imports utils
    └── utils.py      # has no other local imports
Here is how you would use py-dependency-mapper:

Python

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
# Expected output:
# {
#   '/path/to/project/my_app/__init__.py': 'e3b0c442...',
#   '/path/to/project/my_app/main.py': '...',
#   '/path/to/project/my_app/utils.py': '...'
# }
📚 API
build_dependency_map(source_root: str, include_paths: List[str], filter_prefixes: List[str]) -> Dict[str, ProjectFile]
Scans the project and builds the dependency map.

source_root: Absolute path to the root of your source code.

include_paths: A list of directories or files (relative to source_root) to begin the scan from.

filter_prefixes: A list of module prefixes to include in the analysis (e.g., ["my_app"]).

get_dependency_graph(dependency_map: Dict, entry_point: str) -> Dict[str, str]
From the pre-built map, gets the dependency sub-graph for a specific entry point.

dependency_map: The dictionary returned by build_dependency_map.

entry_point: The absolute path to the initial .py file.


📜 License
This project is licensed under the MIT License. See the LICENSE file for more details.

🙏 Acknowledgements
This tool would not be possible without the incredible work of the team behind the Ruff project, whose high-performance parser is the heart of this analyzer. Ruff's license can be found in the licenses/LICENSE-RUFF.md file.