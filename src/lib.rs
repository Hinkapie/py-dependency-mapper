use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::Bound;
use serde::Deserialize;
use serde_json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;
mod helpers;
use helpers::imports_from_source;

#[pyclass]
#[derive(Clone, Debug)]
struct ProjectFile {
    #[pyo3(get)]
    hash: String,
    #[pyo3(get)]
    project_imports: Vec<String>,
    #[pyo3(get)]
    stdlib_imports: Vec<String>,
    #[pyo3(get)]
    third_party_imports: Vec<String>,
}

#[pyclass]
#[derive(Clone, Debug)]
struct GraphFileResult {
    #[pyo3(get)]
    hash: String,
    #[pyo3(get)]
    stdlib_imports: Vec<String>,
    #[pyo3(get)]
    third_party_imports: Vec<String>,
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct PipPackageInfo {
    #[pyo3(get)]
    pub version: String,
    #[pyo3(get)]
    pub installed_paths: Vec<String>,
    #[pyo3(get)]
    pub dependencies: Vec<String>,
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct PipMetadata {
    #[pyo3(get)]
    pub import_to_pip_map: HashMap<String, String>,
    #[pyo3(get)]
    pub pip_package_info_map: HashMap<String, PipPackageInfo>,
    #[pyo3(get)]
    pub extra_dependencies_map: HashMap<String, Vec<String>>,
    #[pyo3(get)]
    pub extra_paths_map: HashMap<String, Vec<String>>,
}

#[derive(Deserialize, Debug, Default)]
struct ManualMappings {
    #[serde(default)]
    import_mappings: HashMap<String, String>,
    #[serde(default)]
    extra_dependencies: HashMap<String, Vec<String>>,
    #[serde(default)]
    extra_package_paths: HashMap<String, Vec<String>>,
}

#[derive(Deserialize, Debug)]
struct PackageDetails {
    version: String,
    dependencies: BTreeMap<String, PackageDetails>,
}

struct PipAnalyzer<'a> {
    site_packages: &'a Path,
    import_to_pip_map: HashMap<String, String>,
    pip_package_info_map: HashMap<String, PipPackageInfo>,
}

impl<'a> PipAnalyzer<'a> {
    fn new(site_packages: &'a Path) -> Self {
        PipAnalyzer {
            site_packages,
            import_to_pip_map: HashMap::new(),
            pip_package_info_map: HashMap::new(),
        }
    }

    fn process_package(&mut self, package_name: &str, package_details: &PackageDetails) {
        if let Some(existing_info) = self.pip_package_info_map.get(package_name) {
            if !existing_info.dependencies.is_empty() && package_details.dependencies.is_empty() {
                return;
            }
        }

        let mut importables = HashSet::new();
        let mut installed_artifact_paths = HashSet::new();
        let dependencies: Vec<String> = package_details.dependencies.keys().cloned().collect();

        if let Some(dist_dir) = find_dist_info_dir(package_name, &package_details.version, self.site_packages) {
            if let Ok(record_content) = fs::read_to_string(dist_dir.join("RECORD")) {
                for line in record_content.lines() {
                    if let Some(path_str) = line.split(',').next() {
                        if path_str.contains(".dist-info/") { continue; }
                        if let Some(top_level) = path_str.split('/').next() {
                            if top_level.is_empty() { continue; }

                            if top_level == "bin" {
                                installed_artifact_paths.insert(path_str.to_string());
                            } else {
                                installed_artifact_paths.insert(top_level.to_string());
                            }
                        }
                    }
                }
            }

            if let Ok(top_level_content) = fs::read_to_string(dist_dir.join("top_level.txt")) {
                for name in top_level_content.lines() {
                    if !name.trim().is_empty() { importables.insert(name.trim().to_string()); }
                }
            } else if !installed_artifact_paths.is_empty() {
                for path in &installed_artifact_paths {
                    let import_name = path.strip_suffix(".py").unwrap_or(path);
                    if !import_name.contains('/') {
                        importables.insert(import_name.to_string());
                    }
                }
            }
        }

        for name in &importables {
            self.import_to_pip_map.entry(name.clone()).or_insert_with(|| package_name.to_string());
        }

        let package_info = PipPackageInfo {
            version: package_details.version.clone(),
            installed_paths: installed_artifact_paths.into_iter().collect(),
            dependencies,
        };
        self.pip_package_info_map.insert(package_name.to_string(), package_info);

        for (dep_name, dep_details) in &package_details.dependencies {
            self.process_package(dep_name, dep_details);
        }
    }

    fn finalize(self) -> (HashMap<String, String>, HashMap<String, PipPackageInfo>) {
        (self.import_to_pip_map, self.pip_package_info_map)
    }
}


#[pyfunction]
#[pyo3(signature = (dependency_tree_json_path, site_packages_path, manual_mapping_path=None))]
pub fn build_pip_metadata(
    dependency_tree_json_path: &str,
    site_packages_path: &str,
    manual_mapping_path: Option<String>,
) -> PyResult<PipMetadata> {

    let site_packages = PathBuf::from(site_packages_path);

    let json_content = fs::read_to_string(dependency_tree_json_path)?;
    let packages: BTreeMap<String, PackageDetails> = serde_json::from_str(&json_content)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

    let mut analyzer = PipAnalyzer::new(&site_packages);

    let mut manual_import_mappings = HashMap::new();
    let mut manual_extra_deps = HashMap::new();
    let mut manual_extra_paths = HashMap::new();

    if let Some(path_str) = manual_mapping_path {
        let path = PathBuf::from(path_str);
        if path.is_file() {
            let content = fs::read_to_string(path)?;
            let mappings: ManualMappings = toml::from_str(&content)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
            manual_import_mappings = mappings.import_mappings;
            manual_extra_deps = mappings.extra_dependencies;
            manual_extra_paths = mappings.extra_package_paths;
        }
    }

    analyzer.import_to_pip_map.extend(manual_import_mappings);

    for (package_name, package_details) in &packages {
        analyzer.process_package(package_name, package_details);
    }

    let (import_map, package_info_map) = analyzer.finalize();

    Ok(PipMetadata {
        import_to_pip_map: import_map,
        pip_package_info_map: package_info_map,
        extra_dependencies_map: manual_extra_deps,
        extra_paths_map: manual_extra_paths,
    })
}


#[pyfunction]
pub fn resolve_package_set(
    direct_packages: Vec<String>,
    pip_metadata: &Bound<'_, PyAny>,
) -> PyResult<HashMap<String, PipPackageInfo>> {
    let metadata: PyRef<PipMetadata> = pip_metadata.extract()?;
    let all_packages_info = &metadata.pip_package_info_map;
    let extra_deps_map = &metadata.extra_dependencies_map;
    

    let mut final_package_set = HashSet::new();
    let mut processing_stack = direct_packages;

    while let Some(package_name) = processing_stack.pop() {
        if !final_package_set.insert(package_name.clone()) {
            continue; 
        }

        if let Some(package_info) = all_packages_info.get(&package_name) {
            for dep in &package_info.dependencies {
                processing_stack.push(dep.clone());
            }
        }
        
        if let Some(extra_deps) = extra_deps_map.get(&package_name) {
            for extra_dep in extra_deps {
                processing_stack.push(extra_dep.clone());
            }
        }
    }

    let resolved_map = final_package_set
        .into_iter()
        .filter_map(|name| all_packages_info.get(&name).map(|info| (name, info.clone())))
        .collect();
    Ok(resolved_map)
}

#[pyfunction]
#[pyo3(signature = (source_root, project_module_prefixes, include_paths, stdlib_list_path=None))]
fn build_dependency_map(
    source_root: &str,
    project_module_prefixes: Vec<String>,
    include_paths: Vec<String>,
    stdlib_list_path: Option<String>,
) -> PyResult<HashMap<String, ProjectFile>> {
    let start_time = Instant::now();
    let source_root_path = PathBuf::from(source_root);
    
    let mut project_file_map = HashMap::with_capacity(4096);
    let mut module_resolution_cache: HashMap<String, Option<PathBuf>> = HashMap::with_capacity(1024);
    let mut package_init_cache: HashMap<String, Vec<PathBuf>> = HashMap::with_capacity(1024);

    let stdlib_modules = if let Some(path) = stdlib_list_path {
        helpers::load_stdlib_from_file(&path)?
    } else {
        HashSet::new()
    };

    for path_str in &include_paths {
        let full_path = source_root_path.join(path_str);
        if full_path.is_dir() {
            for entry in WalkDir::new(full_path).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "py") {
                    parse_file_imports(
                        path, &source_root_path, &project_module_prefixes, &stdlib_modules,
                        &mut project_file_map, &mut module_resolution_cache, &mut package_init_cache,
                    );
                }
            }
        } else if full_path.is_file() {
            parse_file_imports(
                &full_path, &source_root_path, &project_module_prefixes, &stdlib_modules,
                &mut project_file_map, &mut module_resolution_cache, &mut package_init_cache,
            );
        }
    }

    let duration = start_time.elapsed();
    println!(
        "✅ Dependency tree built: {} files in {:.4}s | Include Paths: {:?} | Filter for: {:?}",
        project_file_map.len(),
        duration.as_secs_f64(),
        include_paths,
        project_module_prefixes,
    );
    
    Ok(project_file_map)
}

#[pyfunction]
fn get_dependency_graph(
    dependency_map: &Bound<'_, PyDict>,
    entry_point: &str,
) -> PyResult<HashMap<String, GraphFileResult>> {
    let entry_point_path = fs::canonicalize(entry_point)?.to_string_lossy().into_owned();
    
    let mut resolved_file_map = HashMap::with_capacity(64);
    let mut stack: Vec<String> = vec![entry_point_path];
    let mut seen: HashSet<String> = HashSet::with_capacity(128);

    while let Some(current_path) = stack.pop() {
        if !seen.insert(current_path.clone()) {
            continue;
        }
        if let Some(info_obj) = dependency_map.get_item(&current_path)? {
            let info = info_obj.extract::<PyRef<ProjectFile>>()?;
            let result = GraphFileResult {
                hash: info.hash.clone(),
                stdlib_imports: info.stdlib_imports.clone(),
                third_party_imports: info.third_party_imports.clone(),
            };
            resolved_file_map.insert(current_path, result);
            for import_path in &info.project_imports {
                stack.push(import_path.clone());
            }
        }
    }
    Ok(resolved_file_map)
}

#[pyfunction]
fn find_dependents(
    dependency_map: &Bound<'_, PyDict>,
    changed_file_paths: Vec<String>,
) -> PyResult<HashSet<String>> {
    
    let mut reverse_graph: HashMap<String, Vec<String>> = HashMap::new();
    
    for (importer_path, value) in dependency_map {
        let importer = importer_path.extract::<String>()?;
        let project_file: PyRef<ProjectFile> = value.extract()?;
        
        for dependency in &project_file.project_imports {
            reverse_graph
                .entry(dependency.clone())
                .or_default()
                .push(importer.clone());
        }
    }

    let mut dependents = HashSet::new();
    
    let mut stack = changed_file_paths; 

    while let Some(current_file) = stack.pop() {
        if let Some(consumers) = reverse_graph.get(&current_file) {
            for consumer in consumers {
                if dependents.insert(consumer.clone()) {
                    stack.push(consumer.clone());
                }
            }
        }
    }

    Ok(dependents)
}

#[pymodule]
fn py_dependency_mapper<'py>(_py: Python<'py>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ProjectFile>()?;
    m.add_class::<GraphFileResult>()?;
    m.add_class::<PipMetadata>()?;
    m.add_class::<PipPackageInfo>()?;
    m.add_function(wrap_pyfunction!(build_dependency_map, m)?)?;
    m.add_function(wrap_pyfunction!(get_dependency_graph, m)?)?;
    m.add_function(wrap_pyfunction!(build_pip_metadata, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_package_set, m)?)?;
    m.add_function(wrap_pyfunction!(find_dependents, m)?)?;
    Ok(())
}

fn normalize_pkg_name(name: &str) -> String {
    name.to_lowercase().replace('-', "_")
}

fn find_dist_info_dir(
    package_name: &str,
    version: &str,
    site_packages: &Path,
) -> Option<PathBuf> {
    
    let mut possible_names = HashSet::new();
    
    possible_names.insert(package_name.to_string());
    possible_names.insert(package_name.replace('-', "_"));
    possible_names.insert(package_name.replace('-', "."));

    let mut c = package_name.chars();
    let capitalized_name = match c.next() {
        None => package_name.to_string(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    };
    possible_names.insert(capitalized_name.clone());
    possible_names.insert(capitalized_name.replace('-', "_"));
    possible_names.insert(capitalized_name.replace('-', "."));

    for name in possible_names {
        let dir_name = format!("{}-{}.dist-info", name, version);
        let path = site_packages.join(&dir_name);
        if path.is_dir() {
            return Some(path);
        }
    }

    let normalized_input_name = normalize_pkg_name(package_name);
    let version_suffix = format!("-{}.dist-info", version); 

    let entries = match fs::read_dir(site_packages) {
        Ok(entries) => entries,
        Err(_) => return None, 
    };
    
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }

        let dir_name_str = match entry.file_name().into_string() {
            Ok(s) => s,
            Err(_) => continue,
        };

        if dir_name_str.ends_with(&version_suffix) {
            
            if let Some(actual_name) = dir_name_str.strip_suffix(&version_suffix) {
                
                let normalized_actual_name = normalize_pkg_name(actual_name);

                if normalized_actual_name == normalized_input_name {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn parse_file_imports(
    path: &Path,
    source_root_path: &Path,
    project_module_prefixes: &[String],
    stdlib_modules: &HashSet<String>,
    project_file_map: &mut HashMap<String, ProjectFile>,
    module_resolution_cache: &mut HashMap<String, Option<PathBuf>>,
    package_init_cache: &mut HashMap<String, Vec<PathBuf>>,
) {
    let path_str = path.to_string_lossy().into_owned();
    if project_file_map.contains_key(&path_str) { return; }

    if let Ok(content_bytes) = fs::read(path) {
        let mut hasher = Sha256::new();
        hasher.update(&content_bytes);
        let hash = hex::encode(hasher.finalize());

        let mut resolved_project_imports = HashSet::new();
        let mut stdlib_imports = HashSet::new();
        let mut third_party_imports = HashSet::new();

        if let Ok(content_str) = std::str::from_utf8(&content_bytes) {
            let import_strings = imports_from_source(content_str);
            for module in import_strings {
                let base_module = module.split('.').next().unwrap_or(&module);

                if project_module_prefixes.iter().any(|prefix| module.starts_with(prefix)) {
                    for p in helpers::find_package_inits_in_path_seq(&module, source_root_path, package_init_cache) {
                        resolved_project_imports.insert(p.to_string_lossy().into_owned());
                    }
                    if let Some(p) = helpers::resolve_module_in_project_seq(&module, source_root_path, module_resolution_cache) {
                        resolved_project_imports.insert(p.to_string_lossy().into_owned());
                    }
                } else if stdlib_modules.contains(base_module) {
                    stdlib_imports.insert(base_module.to_string());
                } else {
                    third_party_imports.insert(base_module.to_string());
                }
            }
        }
        project_file_map.insert(path_str, ProjectFile {
            hash,
            project_imports: resolved_project_imports.into_iter().collect(),
            stdlib_imports: stdlib_imports.into_iter().collect(),
            third_party_imports: third_party_imports.into_iter().collect(),
        });
    }
}










#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;
    use pyo3::types::PyDict;
   
    fn mock_file(py: Python, imports: Vec<&str>) -> PyObject {
        let file = ProjectFile {
            hash: "dummy".to_string(),
            project_imports: imports.iter().map(|s| s.to_string()).collect(),
            stdlib_imports: vec![],
            third_party_imports: vec![],
        };
        Py::new(py, file).unwrap().into_any()
    }

    #[test]
    fn test_find_dependents_logic() {
        pyo3::prepare_freethreaded_python();
        
        Python::with_gil(|py| {
            let map = PyDict::new(py);

            map.set_item("file_a.py", mock_file(py, vec!["file_b.py"])).unwrap();
            map.set_item("file_b.py", mock_file(py, vec![])).unwrap();
            
            map.set_item("file_c.py", mock_file(py, vec!["file_d.py"])).unwrap();
            map.set_item("file_d.py", mock_file(py, vec![])).unwrap();

            let result = find_dependents(&map, vec![
                "file_b.py".to_string(), 
                "file_d.py".to_string()
            ]).unwrap();

            assert!(result.contains("file_a.py"));
            assert!(result.contains("file_c.py"));
            assert_eq!(result.len(), 2);
        });
    }

    #[test]
    fn test_normalize_pkg_name() {
        assert_eq!(normalize_pkg_name("CairoSVG"), "cairosvg");
        assert_eq!(normalize_pkg_name("google-api-core"), "google_api_core");
        assert_eq!(normalize_pkg_name("Babel"), "babel");
    }

    #[test]
    fn test_find_dist_info_dir_fast_path() {
        let dir = tempdir().unwrap();
        let site_packages = dir.path();
        
        let pkg_dir = site_packages.join("requests-2.32.5.dist-info");
        fs::create_dir(&pkg_dir).unwrap();

        let found = find_dist_info_dir("requests", "2.32.5", site_packages);
        
        let found_canon = found.map(|p| fs::canonicalize(p).unwrap());
        let pkg_dir_canon = Some(fs::canonicalize(pkg_dir).unwrap());

        assert_eq!(found_canon, pkg_dir_canon);
    }
    
    #[test]
    fn test_find_dist_info_dir_slow_path_capitalized() {
        let dir = tempdir().unwrap();
        let site_packages = dir.path();

        let pkg_dir = site_packages.join("CairoSVG-2.7.0.dist-info");
        fs::create_dir(&pkg_dir).unwrap();

        let found = find_dist_info_dir("cairosvg", "2.7.0", site_packages);
        
        let found_canon = found.map(|p| fs::canonicalize(p).unwrap());
        let pkg_dir_canon = Some(fs::canonicalize(pkg_dir).unwrap());

        assert_eq!(found_canon, pkg_dir_canon);
    }

    #[test]
    fn test_find_dist_info_dir_not_found() {
        let dir = tempdir().unwrap();
        let site_packages = dir.path();
        
        let found = find_dist_info_dir("nonexistent", "1.0.0", site_packages);
        assert_eq!(found, None);
    }

    #[test]
    fn test_pip_analyzer_process_package_with_record() {
        let dir = tempdir().unwrap();
        let site_packages = dir.path();

        let dist_info = site_packages.join("test_pkg-1.0.0.dist-info");
        fs::create_dir(&dist_info).unwrap();

        let record_path = dist_info.join("RECORD");
        let mut record_file = File::create(record_path).unwrap();
        writeln!(record_file, "test_pkg/__init__.py,sha256=...,100").unwrap();
        writeln!(record_file, "bin/test-cli,sha256=...,200").unwrap();
        writeln!(record_file, "test_pkg-1.0.0.dist-info/METADATA,sha256=...,300").unwrap();

        let mut analyzer = PipAnalyzer::new(site_packages);
        let details = PackageDetails {
            version: "1.0.0".to_string(),
            dependencies: BTreeMap::new(),
        };

        analyzer.process_package("test-pkg", &details);

        let info = analyzer.pip_package_info_map.get("test-pkg").unwrap();
        assert_eq!(info.version, "1.0.0");
        assert!(info.installed_paths.contains(&"test_pkg".to_string()));
        assert!(info.installed_paths.contains(&"bin/test-cli".to_string()));
        assert!(!info.installed_paths.iter().any(|p| p.contains(".dist-info")));
        assert_eq!(analyzer.import_to_pip_map.get("test_pkg"), Some(&"test-pkg".to_string()));
    }

    #[test]
    fn test_recursive_dependency_analysis() {
        let dir = tempdir().unwrap();
        let site_packages = dir.path();

        let setup_pkg = |name: &str, version: &str| {
            let dist = site_packages.join(format!("{}-{}.dist-info", name, version));
            fs::create_dir(&dist).unwrap();
            let mut record = File::create(dist.join("RECORD")).unwrap();
            writeln!(record, "{}/__init__.py,sha256=...,100", name).unwrap();
        };

        setup_pkg("pkg_a", "1.0");
        setup_pkg("pkg_b", "2.0");
        setup_pkg("pkg_c", "3.0");

        let deps_c = BTreeMap::new(); 
        let details_c = PackageDetails { version: "3.0".to_string(), dependencies: deps_c };

        let mut deps_b = BTreeMap::new();
        deps_b.insert("pkg_c".to_string(), details_c); 
        let details_b = PackageDetails { version: "2.0".to_string(), dependencies: deps_b };

        let mut deps_a_real = BTreeMap::new();
        deps_a_real.insert("pkg_b".to_string(), details_b);
        
        let details_a = PackageDetails { version: "1.0".to_string(), dependencies: deps_a_real };

        let mut analyzer = PipAnalyzer::new(site_packages);
    
        analyzer.process_package("pkg_a", &details_a);
        assert!(analyzer.pip_package_info_map.contains_key("pkg_a"));
        assert!(analyzer.pip_package_info_map.contains_key("pkg_b"));
        assert!(analyzer.pip_package_info_map.contains_key("pkg_c"));
        assert_eq!(analyzer.pip_package_info_map.get("pkg_c").unwrap().version, "3.0");
    }

    #[test]
    fn test_build_pip_metadata_with_manual_mappings() {
        let dir = tempdir().unwrap();
        let site_packages = dir.path();
        
        let json_path = dir.path().join("tree.json");
        fs::write(&json_path, "{}").unwrap(); 

        let toml_path = dir.path().join("mappings.toml");
        let toml_content = r#"
            [import_mappings]
            "slack" = "slackclient"

            [extra_dependencies]
            "pydantic" = ["email-validator"]

            [extra_package_paths]
            "gremlinpython" = ["bin", "lib"]
        "#;
        fs::write(&toml_path, toml_content).unwrap();

        let metadata = build_pip_metadata(
            json_path.to_str().unwrap(),
            site_packages.to_str().unwrap(),
            Some(toml_path.to_str().unwrap().to_string())
        ).unwrap();

        assert_eq!(metadata.import_to_pip_map.get("slack"), Some(&"slackclient".to_string()));

        let extra_deps = metadata.extra_dependencies_map.get("pydantic").unwrap();
        assert!(extra_deps.contains(&"email-validator".to_string()));

        let extra_paths = metadata.extra_paths_map.get("gremlinpython").unwrap();
        assert!(extra_paths.contains(&"bin".to_string()));
        assert!(extra_paths.contains(&"lib".to_string()));
    }    
}