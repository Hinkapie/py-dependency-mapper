use pyo3::prelude::*;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Stmt, Expr};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn load_stdlib_from_file(path: &str) -> PyResult<HashSet<String>> {
    let content = fs::read_to_string(path)?;
    let cleaned_content = content
        .trim()
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(&content);

    let modules: HashSet<String> = cleaned_content
        .split(',')
        .map(|s| s.trim().trim_matches('\'').to_string())
        .collect();
        
    Ok(modules)
}

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

fn get_dotted_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(attr) => {
            let base = get_dotted_name(&attr.value)?;
            Some(format!("{}.{}", base, attr.attr))
        }
        _ => None,
    }
}

pub(super) fn imports_from_source(source: &str) -> Vec<String> {
    let parsed = match ruff_python_parser::parse_module(source) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    #[derive(Default)]
    struct ImportAndUsageVisitor {
        imports: HashSet<String>,
        aliases: HashMap<String, String>,
    }

    impl<'ast> Visitor<'ast> for ImportAndUsageVisitor {
        fn visit_stmt(&mut self, stmt: &'ast Stmt) {
            match stmt {
                Stmt::Import(i) => {
                    for a in &i.names {
                        self.imports.insert(a.name.to_string());
                        if let Some(asname) = &a.asname {
                            self.aliases.insert(asname.to_string(), a.name.to_string());
                        }
                    }
                }
                Stmt::ImportFrom(i) => {
                    if i.level == 0 {
                        if let Some(m) = &i.module {
                            self.imports.insert(m.to_string());
                            for a in &i.names {
                                if a.name.to_string() != "*" {
                                    let full_name = format!("{}.{}", m, a.name);
                                    self.imports.insert(full_name.clone());
                                    if let Some(asname) = &a.asname {
                                        self.aliases.insert(asname.to_string(), full_name);
                                    } else {
                                        self.aliases.insert(a.name.to_string(), full_name);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            visitor::walk_stmt(self, stmt);
        }

        fn visit_expr(&mut self, expr: &'ast Expr) {
            match expr {
                Expr::Attribute(attr) => {
                    if let Some(base_name) = get_dotted_name(&attr.value) {
                        let resolved_base = self.aliases.get(&base_name).unwrap_or(&base_name);
                        if self.imports.contains(resolved_base) || self.aliases.contains_key(&base_name) {
                            let detected_usage = format!("{}.{}", resolved_base, attr.attr);
                            self.imports.insert(detected_usage);
                        }
                    }
                }
                _ => {}
            }
            visitor::walk_expr(self, expr);
        }
    }

    let mut visitor = ImportAndUsageVisitor::default();
    let module = parsed.into_syntax();
    visitor.visit_body(&module.body);
    
    visitor.imports.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::tempdir;

    #[test]
    fn test_imports_from_source_basic() {
        let source_code = r#"
import os
import sys as system
from collections import namedtuple
import my_package.module
        "#;
        
        let imports = imports_from_source(source_code);
        let imports_set: HashSet<_> = imports.into_iter().collect();

        assert!(imports_set.contains("os"));
        assert!(imports_set.contains("sys"));
        assert!(imports_set.contains("collections"));
        assert!(imports_set.contains("collections.namedtuple"));
        assert!(imports_set.contains("my_package.module")); 
    }

    #[test]
    fn test_imports_from_source_edge_cases() {
        let source_relative = "from . import sibling";
        let imports = imports_from_source(source_relative);
        assert!(imports.is_empty(), "Should ignore relative imports");

        let source_wildcard = "from os import *";
        let imports = imports_from_source(source_wildcard);
        assert!(imports.contains(&"os".to_string()));
        assert!(!imports.iter().any(|s| s.contains('*')));

        let source_invalid = "import "; 
        let imports = imports_from_source(source_invalid);
        assert!(imports.is_empty(), "Should return empty list on syntax error");
    }

    #[test]
    fn test_imports_from_source_attribute_usage() {
        let source_code = r#"
import pydantic.v1 as pydantic
import openpyxl.drawing.image

class User:
    # usage with alias
    email: pydantic.EmailStr
    
# direct usage of deep module
img = openpyxl.drawing.image.Image('test.png')
        "#;
        
        let imports = imports_from_source(source_code);
        let imports_set: HashSet<_> = imports.into_iter().collect();

        assert!(imports_set.contains("pydantic.v1.EmailStr"));
        assert!(imports_set.contains("openpyxl.drawing.image.Image"));
        assert!(imports_set.contains("pydantic.v1"));
        assert!(imports_set.contains("openpyxl.drawing.image"));
    }

    #[test]
    fn test_find_package_inits() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let pkg_dir = root.join("pkg");
        fs::create_dir(&pkg_dir).unwrap();
        File::create(pkg_dir.join("__init__.py")).unwrap();

        let mut cache = HashMap::new();
        
        let inits = find_package_inits_in_path_seq("pkg.submodule", root, &mut cache);
        
        assert_eq!(inits.len(), 1);
        assert_eq!(inits[0], pkg_dir.join("__init__.py"));

        let inits_cached = find_package_inits_in_path_seq("pkg.submodule", root, &mut cache);
        assert_eq!(inits_cached.len(), 1);
    }

    #[test]
    fn test_resolve_module_file() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let utils_path = root.join("utils.py");
        File::create(&utils_path).unwrap();

        let mut cache = HashMap::new();
        
        let result = resolve_module_in_project_seq("utils", root, &mut cache);
        assert_eq!(result, Some(utils_path));
        
        let result_none = resolve_module_in_project_seq("missing", root, &mut cache);
        assert_eq!(result_none, None);
    }

    #[test]
    fn test_resolve_package_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        
        let pkg_dir = root.join("mypkg");
        fs::create_dir(&pkg_dir).unwrap();
        let init_path = pkg_dir.join("__init__.py");
        File::create(&init_path).unwrap();

        let mut cache = HashMap::new();
        
        let result = resolve_module_in_project_seq("mypkg", root, &mut cache);
        assert_eq!(result, Some(init_path));
    }

    #[test]
    fn test_load_stdlib_from_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("stdlib.txt");
        fs::write(&file_path, "['os', 'sys', 're']").unwrap();
        
        let stdlib = load_stdlib_from_file(file_path.to_str().unwrap()).unwrap();
        
        assert!(stdlib.contains("os"));
        assert!(stdlib.contains("sys"));
        assert!(stdlib.contains("re"));
        assert!(!stdlib.contains("requests"));
    }
}