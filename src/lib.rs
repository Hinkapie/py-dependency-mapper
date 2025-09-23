use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use pyo3::Bound;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

use hex;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::Stmt;
use ruff_python_parser::parse_module;
use sha2::{Digest, Sha256};

use helpers::*;

#[pyclass]
#[derive(Clone, Debug)]
struct ProjectFile {
    #[pyo3(get)]
    hash: String,
    #[pyo3(get)]
    imports: Vec<String>,
}

fn analyze_and_dependency_map_file(
    path: &Path,
    source_root_path: &Path,
    filter_prefixes: &[String],
    dependency_map: &mut HashMap<String, ProjectFile>,
    resolution_cache: &mut HashMap<String, Option<PathBuf>>,
    inits_cache: &mut HashMap<String, Vec<PathBuf>>,
) {
    let canonical_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let path_str = canonical_path.to_string_lossy().into_owned();

    if dependency_map.contains_key(&path_str) {
        return;
    }

    if let Ok(content_bytes) = fs::read(&canonical_path) {
        let mut hasher = Sha256::new();
        hasher.update(&content_bytes);
        let hash = hex::encode(hasher.finalize());

        let mut resolved_imports = HashSet::new();
        if let Ok(content_str) = std::str::from_utf8(&content_bytes) {
            let import_strings = imports_from_source(content_str);
            for module in import_strings.into_iter().filter(|m| {
                filter_prefixes.iter().any(|prefix| m.starts_with(prefix))
            }) {
                let init_paths =
                    find_package_inits_in_path_seq(&module, &source_root_path, inits_cache);
                for p in init_paths {
                    resolved_imports.insert(p.to_string_lossy().into_owned());
                }
                if let Some(resolved_path) =
                    resolve_module_in_project_seq(&module, &source_root_path, resolution_cache)
                {
                    resolved_imports.insert(resolved_path.to_string_lossy().into_owned());
                }
            }
        }

        dependency_map.insert(
            path_str,
            ProjectFile {
                hash,
                imports: resolved_imports.into_iter().collect(),
            },
        );
    }
}

#[pyfunction]
fn build_dependency_map(
    source_root: &str,
    filter_prefixes: Vec<String>,
    include_paths: Vec<String>,
) -> PyResult<HashMap<String, ProjectFile>> {
    let start_time = Instant::now();

    let source_root_path = fs::canonicalize(source_root).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyFileNotFoundError, _>(format!(
            "Project root not found: {} ({})",
            source_root, e
        ))
    })?;

    let mut dependency_map = HashMap::with_capacity(4096);
    let mut resolution_cache: HashMap<String, Option<PathBuf>> = HashMap::with_capacity(1024);
    let mut inits_cache: HashMap<String, Vec<PathBuf>> = HashMap::with_capacity(1024);

    for path_str in &include_paths {
        let full_path = source_root_path.join(&path_str);

        if full_path.is_dir() {
            for entry in WalkDir::new(full_path).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "py") {
                    analyze_and_dependency_map_file(
                        path,
                        &source_root_path,
                        &filter_prefixes,
                        &mut dependency_map,
                        &mut resolution_cache,
                        &mut inits_cache,
                    );
                }
            }
        } else if full_path.is_file() {
            analyze_and_dependency_map_file(
                &full_path,
                &source_root_path,
                &filter_prefixes,
                &mut dependency_map,
                &mut resolution_cache,
                &mut inits_cache,
            );
        }
    }

    let duration = start_time.elapsed();
    println!(
        "✅ Dependency tree built: {} files in {:.4}s | Include Paths: {:?} | Filter for: {:?}",
        dependency_map.len(),
        duration.as_secs_f64(),
        include_paths,
        filter_prefixes,
    );
    Ok(dependency_map)
}

#[pyfunction]
fn get_dependency_graph(
    dependency_map: &Bound<'_, PyDict>,
    entry_point: &str,
) -> PyResult<HashMap<String, String>> {
    let canonical_handler = fs::canonicalize(entry_point)
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyFileNotFoundError, _>(format!(
                "Handler file not found: {} ({})",
                entry_point, e
            ))
        })?
        .to_string_lossy()
        .into_owned();

    let mut final_deps: HashMap<String, String> = HashMap::with_capacity(64);
    let mut stack: Vec<String> = vec![canonical_handler];
    let mut seen: HashSet<String> = HashSet::with_capacity(128);

    while let Some(current_path) = stack.pop() {
        if !seen.insert(current_path.clone()) {
            continue;
        }
        if let Some(info_obj) = dependency_map.get_item(&current_path)? {
            let info = info_obj.extract::<PyRef<ProjectFile>>()?;
            final_deps.insert(current_path, info.hash.clone());
            for import_path in &info.imports {
                stack.push(import_path.clone());
            }
        }
    }
    Ok(final_deps)
}

#[pymodule]
fn py_dependency_mapper<'py>(_py: Python<'py>, m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<ProjectFile>()?;
    m.add_function(wrap_pyfunction!(build_dependency_map, m)?)?;
    m.add_function(wrap_pyfunction!(get_dependency_graph, m)?)?;
    Ok(())
}

mod helpers {
    use super::*;

    pub(super) fn find_package_inits_in_path_seq(
        module: &str,
        source_root: &Path,
        cache: &mut HashMap<String, Vec<PathBuf>>,
    ) -> Vec<PathBuf> {
        if let Some(cached) = cache.get(module) {
            return cached.clone();
        }
        let mut inits = Vec::new();
        let segments: Vec<&str> = module.split('.').collect();
        if segments.len() > 1 {
            let mut current_path = source_root.to_path_buf();
            for segment in &segments[..segments.len() - 1] {
                current_path.push(segment);
                let init_path = current_path.join("__init__.py");
                if init_path.exists() {
                    inits.push(init_path);
                }
            }
        }
        cache.insert(module.to_string(), inits.clone());
        inits
    }

    pub(super) fn resolve_module_in_project_seq(
        module: &str,
        source_root: &Path,
        cache: &mut HashMap<String, Option<PathBuf>>,
    ) -> Option<PathBuf> {
        if let Some(cached) = cache.get(module) {
            return cached.clone();
        }
        let rel_path = module.replace('.', "/");
        let result = {
            let pkg_init = source_root.join(&rel_path).join("__init__.py");
            if pkg_init.exists() {
                Some(pkg_init)
            } else {
                let py_file = source_root.join(&rel_path).with_extension("py");
                if py_file.exists() {
                    Some(py_file)
                } else {
                    None
                }
            }
        };
        cache.insert(module.to_string(), result.clone());
        result
    }

    pub(super) fn imports_from_source(source: &str) -> Vec<String> {
        let parsed = match parse_module(source) {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };
        #[derive(Default)]
        struct ImportVisitor {
            imports: Vec<String>,
        }
        impl<'ast> Visitor<'ast> for ImportVisitor {
            fn visit_stmt(&mut self, stmt: &'ast Stmt) {
                match stmt {
                    Stmt::Import(i) => {
                        for a in &i.names {
                            self.imports.push(a.name.to_string());
                        }
                    }
                    Stmt::ImportFrom(i) => {
                        if i.level == 0 {
                            if let Some(m) = &i.module {
                                self.imports.push(m.to_string());
                                for a in &i.names {
                                    if a.name.to_string() != "*" {
                                        self.imports.push(format!("{}.{}", m, a.name));
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
                visitor::walk_stmt(self, stmt);
            }
        }
        let mut visitor = ImportVisitor::default();
        let module = parsed.into_syntax();
        visitor.visit_body(&module.body);
        visitor.imports
    }
}