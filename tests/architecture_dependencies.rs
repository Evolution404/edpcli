//! Rust AST checks: comments/strings, nested imports and aliases are not text matches.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use syn::{
    visit::{self, Visit},
    UseTree,
};
fn imports(tree: &UseTree, prefix: Vec<String>, out: &mut Vec<(Vec<String>, Option<String>)>) {
    match tree {
        UseTree::Path(path) => {
            let mut next = prefix;
            next.push(path.ident.to_string());
            imports(&path.tree, next, out);
        }
        UseTree::Group(group) => {
            for tree in &group.items {
                imports(tree, prefix.clone(), out);
            }
        }
        UseTree::Name(name) => {
            let mut next = prefix;
            if name.ident != "self" {
                next.push(name.ident.to_string());
            }
            out.push((next, Some(name.ident.to_string())));
        }
        UseTree::Rename(rename) => {
            let mut next = prefix;
            next.push(rename.ident.to_string());
            out.push((next, Some(rename.rename.to_string())));
        }
        UseTree::Glob(_) => out.push((prefix, None)),
    }
}
fn resolve(
    path: &[String],
    module: &[String],
    aliases: &BTreeMap<String, Vec<String>>,
) -> Option<Vec<String>> {
    let mut result = Vec::new();
    let mut offset;
    match path.first()?.as_str() {
        "crate" => offset = 1,
        "self" => {
            result = module.to_vec();
            offset = 1;
        }
        "super" => {
            offset = 0;
            result = module.to_vec();
            while path.get(offset).is_some_and(|part| part == "super") {
                result.pop();
                offset += 1;
            }
        }
        name => {
            result = aliases.get(name)?.clone();
            offset = 1;
        }
    }
    result.extend_from_slice(&path[offset..]);
    Some(result)
}
struct Dependencies {
    module: Vec<String>,
    aliases: BTreeMap<String, Vec<String>>,
    targets: BTreeSet<Vec<String>>,
}
impl<'ast> Visit<'ast> for Dependencies {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut found = Vec::new();
        imports(&item.tree, Vec::new(), &mut found);
        for (path, alias) in found {
            if let Some(target) = resolve(&path, &self.module, &self.aliases) {
                if let Some(alias) = alias {
                    self.aliases.insert(alias, target.clone());
                }
                self.targets.insert(target);
            }
        }
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if let Some((_, items)) = &item.content {
            let aliases = self.aliases.clone();
            self.module.push(item.ident.to_string());
            for _ in 0..2 {
                for child in items {
                    self.visit_item(child);
                }
            }
            self.module.pop();
            self.aliases = aliases;
        }
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let parts: Vec<_> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        if let Some(target) = resolve(&parts, &self.module, &self.aliases) {
            self.targets.insert(target);
        }
        visit::visit_path(self, path);
    }
}
fn dependencies(source: &str, module: Vec<String>) -> BTreeSet<Vec<String>> {
    let file = syn::parse_file(source).expect("valid Rust source");
    let mut dependencies = Dependencies {
        module,
        aliases: BTreeMap::new(),
        targets: BTreeSet::new(),
    };
    // Imports can refer to previously defined aliases or appear after a use site.
    dependencies.visit_file(&file);
    dependencies.visit_file(&file);
    dependencies.targets
}
fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path.to_owned());
        }
        return;
    }
    for entry in fs::read_dir(path).unwrap() {
        collect(&entry.unwrap().path(), out);
    }
}
// Discover logical module paths, including relocated #[path] sources and inline scopes.
// Source registration and layer direction are checked separately below.
fn module_paths(entry: &Path) -> BTreeMap<PathBuf, Vec<String>> {
    fn walk(
        path: &Path,
        module: Vec<String>,
        relocated: bool,
        out: &mut BTreeMap<PathBuf, Vec<String>>,
    ) {
        let path = path.canonicalize().unwrap();
        if out.contains_key(&path) {
            return;
        }
        out.insert(path.clone(), module.clone());
        let ast = syn::parse_file(&fs::read_to_string(&path).unwrap()).unwrap();
        let parent = path.parent().unwrap();
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let dir = if relocated || matches!(stem, "lib" | "main" | "mod") {
            parent.to_owned()
        } else {
            parent.join(stem)
        };
        walk_items(&ast.items, &module, parent, &dir, out);
    }
    fn walk_items(
        items: &[syn::Item],
        module: &[String],
        path_base: &Path,
        dir: &Path,
        out: &mut BTreeMap<PathBuf, Vec<String>>,
    ) {
        for item in items {
            let syn::Item::Mod(item) = item else { continue };
            let mut child = module.to_vec();
            child.push(item.ident.to_string());
            let relocated = item.attrs.iter().find_map(|attr| {
                if !attr.path().is_ident("path") {
                    return None;
                }
                if let syn::Meta::NameValue(value) = &attr.meta {
                    if let syn::Expr::Lit(value) = &value.value {
                        if let syn::Lit::Str(value) = &value.lit {
                            return Some(value.value());
                        }
                    }
                }
                None
            });
            if let Some((_, contents)) = &item.content {
                let inline_dir = relocated
                    .map_or_else(|| dir.join(item.ident.to_string()), |name| dir.join(name));
                walk_items(contents, &child, &inline_dir, &inline_dir, out);
            } else {
                let is_relocated = relocated.is_some();
                let file = if let Some(name) = relocated {
                    path_base.join(name)
                } else {
                    let flat = dir.join(format!("{}.rs", item.ident));
                    if flat.exists() {
                        flat
                    } else {
                        dir.join(item.ident.to_string()).join("mod.rs")
                    }
                };
                assert!(file.exists(), "module source missing: {}", file.display());
                walk(&file, child, is_relocated, out);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(entry, Vec::new(), false, &mut out);
    out
}
#[test]
fn all_production_sources_are_registered_in_the_module_graph() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Walk all cfg branches, including other platforms and internal test modules.
    let mut registered = module_paths(&root.join("src/lib.rs"));
    registered.extend(module_paths(&root.join("src/main.rs")));
    let mut files = Vec::new();
    collect(&root.join("src"), &mut files);
    for path in files {
        assert!(
            registered.contains_key(&path.canonicalize().unwrap()),
            "unregistered production source: {}",
            path.strip_prefix(root).unwrap().display()
        );
    }
}

#[test]
fn all_domain_ports_infrastructure_and_container_modules_obey_layer_direction() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect(&root.join("src"), &mut files);
    let logical_paths = module_paths(&root.join("src/lib.rs"));
    let mut checked = 0;
    for path in files {
        let relative = path.strip_prefix(root.join("src")).unwrap();
        let mut module: Vec<String> = relative
            .components()
            .map(|part| {
                part.as_os_str()
                    .to_string_lossy()
                    .trim_end_matches(".rs")
                    .to_string()
            })
            .collect();
        if module.last().is_some_and(|part| part == "mod") {
            module.pop();
        }
        if let Some(logical) = logical_paths.get(&path.canonicalize().unwrap()) {
            module = logical.clone();
        }
        let Some(first) = module.first().map(String::as_str) else {
            continue;
        };
        let forbidden: &[&str] = match first {
            "domain" | "ports" => &[
                "application",
                "tui",
                "cli",
                "platform",
                "sysinfo",
                "infrastructure",
                "diskio",
            ],
            "infrastructure" | "edpb" => &["application", "tui", "cli"],
            "filesystem" => &["backup_metadata", "application", "tui", "cli"],
            "platform" if relative != Path::new("platform/mod.rs") => {
                &["sysinfo", "application", "tui", "cli"]
            }
            _ => continue,
        };
        checked += 1;
        let source = fs::read_to_string(&path).unwrap();
        for target in dependencies(&source, module.clone()) {
            assert!(
                !target
                    .first()
                    .is_some_and(|part| forbidden.contains(&part.as_str())),
                "{} imports forbidden {}",
                relative.display(),
                target.join("::")
            );
            if first == "infrastructure" && module.get(1).is_some_and(|part| part == "backup_store")
            {
                assert_ne!(
                    target.first().map(String::as_str),
                    Some("diskio"),
                    "storage implementation depends on its facade: {}",
                    relative.display()
                );
            }
        }
    }
    assert!(checked > 30, "new modules must be discovered automatically");
}
#[test]
fn dependency_detector_rejects_nested_alias_and_relative_imports_but_ignores_text() {
    for source in ["use crate::{application::{self as app, error::OperationError}}; fn f() { let _ = app::device::guard_system_disk; }", "pub use super::super::application as alias;", "use crate::application as alias; use alias::error as errors;"] {
        assert!(dependencies(source, vec!["domain".into(), "nested".into()]).iter().any(|path| path.first().is_some_and(|part| part == "application")));
    }
    let source = "// use crate::application;\nconst TEXT: &str = \"crate::application\";";
    assert!(dependencies(source, vec!["domain".into()]).is_empty());
}

#[test]
fn detector_uses_inline_and_relocated_logical_module_paths() {
    struct Temp(PathBuf);
    impl Temp {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("edpcli-module-{}-{unique}", std::process::id());
    let tmp = Temp(std::env::temp_dir().join(name));
    fs::create_dir(&tmp.0).unwrap();
    fs::create_dir(tmp.path().join("elsewhere")).unwrap();
    fs::write(
        tmp.path().join("lib.rs"),
        "mod domain { #[path = \"../elsewhere/nested.rs\"] mod nested; }",
    )
    .unwrap();
    fs::create_dir(tmp.path().join("domain")).unwrap();
    let nested = tmp.path().join("elsewhere/nested.rs");
    fs::write(&nested, "pub use super::super::application as forbidden;").unwrap();
    let paths = module_paths(&tmp.path().join("lib.rs"));
    assert_eq!(
        paths[&nested.canonicalize().unwrap()],
        vec!["domain", "nested"]
    );
    assert!(dependencies(
        &fs::read_to_string(nested).unwrap(),
        vec!["domain".into(), "nested".into()]
    )
    .iter()
    .any(|target| target[0] == "application"));
    assert!(dependencies(
        "mod inner { use super::super::application; }",
        vec!["domain".into()]
    )
    .iter()
    .any(|target| target[0] == "application"));
}
