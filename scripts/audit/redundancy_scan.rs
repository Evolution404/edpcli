//! AST facts for scripts/audit-redundancy.py. No device access or mutations.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
use syn::{
    visit::{self, Visit},
    Item, Meta, Visibility,
};

#[derive(Deserialize)]
struct Input {
    files: Vec<String>,
    roots: Vec<String>,
}
#[derive(Serialize)]
struct Function {
    path: String,
    name: String,
    owner: String,
    public: bool,
    test_only: bool,
    trait_impl: bool,
    ignored_parameters: Vec<String>,
    forwards_to: Option<String>,
}
#[derive(Serialize)]
struct Reference {
    path: String,
    name: String,
    test_only: bool,
}
#[derive(Serialize)]
struct DeclaredSymbol {
    path: String,
    name: String,
    public: bool,
    test_only: bool,
}
#[derive(Default, Serialize)]
struct Facts {
    functions: Vec<Function>,
    declared_symbols: Vec<DeclaredSymbol>,
    reexport_files: BTreeSet<String>,
    references: Vec<Reference>,
    registered: BTreeSet<String>,
    production_files: BTreeSet<String>,
    test_files: BTreeSet<String>,
    missing_modules: BTreeSet<String>,
}
fn requires_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) => {
            let Ok(parts) = list.parse_args_with(
                syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
            ) else {
                return false;
            };
            if list.path.is_ident("all") {
                parts.iter().any(requires_test)
            } else if list.path.is_ident("any") {
                !parts.is_empty() && parts.iter().all(requires_test)
            } else {
                false
            }
        }
        _ => false,
    }
}
fn test_attrs(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg")
                && attr
                    .parse_args::<Meta>()
                    .is_ok_and(|meta| requires_test(&meta)))
    })
}
fn format_names(value: &str, out: &mut BTreeSet<String>) {
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '{' {
            continue;
        }
        if chars.peek() == Some(&'{') {
            chars.next();
            continue;
        }
        let mut placeholder = String::new();
        for ch in chars.by_ref() {
            if ch == '}' {
                break;
            }
            placeholder.push(ch);
        }
        let name = placeholder.split(':').next().unwrap_or("").trim();
        if !name.is_empty() && syn::parse_str::<syn::Ident>(name).is_ok() {
            out.insert(name.into());
        }
    }
}
fn format_macro_name(name: &str) -> bool {
    matches!(
        name,
        "format"
            | "format_args"
            | "write"
            | "writeln"
            | "print"
            | "println"
            | "eprint"
            | "eprintln"
            | "panic"
            | "assert"
            | "assert_eq"
            | "assert_ne"
            | "debug_assert"
            | "debug_assert_eq"
            | "debug_assert_ne"
    )
}
fn format_macro(value: &syn::Macro) -> bool {
    value
        .path
        .segments
        .last()
        .is_some_and(|s| format_macro_name(&s.ident.to_string()))
}
fn token_names(
    tokens: proc_macro2::TokenStream,
    out: &mut BTreeSet<String>,
    capture_formats: bool,
) {
    let tokens: Vec<_> = tokens.into_iter().collect();
    for (index, token) in tokens.iter().enumerate() {
        match token {
            proc_macro2::TokenTree::Ident(name) => {
                out.insert(name.to_string());
            }
            proc_macro2::TokenTree::Group(group) => {
                let nested_format = index >= 2
                    && matches!(&tokens[index - 1], proc_macro2::TokenTree::Punct(p) if p.as_char() == '!')
                    && matches!(&tokens[index - 2], proc_macro2::TokenTree::Ident(name) if format_macro_name(&name.to_string()));
                token_names(group.stream(), out, capture_formats || nested_format);
            }
            proc_macro2::TokenTree::Literal(literal) if capture_formats => {
                if let Ok(value) = syn::parse_str::<syn::LitStr>(&literal.to_string()) {
                    format_names(&value.value(), out);
                }
            }
            _ => (),
        }
    }
}
#[derive(Default)]
struct Names {
    names: BTreeSet<String>,
}
impl<'ast> Visit<'ast> for Names {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.names.extend(
            path.segments
                .iter()
                .map(|segment| segment.ident.to_string()),
        );
        visit::visit_path(self, path);
    }
    fn visit_expr_method_call(&mut self, expr: &'ast syn::ExprMethodCall) {
        self.names.insert(expr.method.to_string());
        visit::visit_expr_method_call(self, expr);
    }
    fn visit_macro(&mut self, value: &'ast syn::Macro) {
        token_names(value.tokens.clone(), &mut self.names, format_macro(value));
    }
}
fn direct_argument(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Path(_) | syn::Expr::Lit(_) => true,
        syn::Expr::Reference(reference) => direct_argument(&reference.expr),
        _ => false,
    }
}
fn forwards(block: &syn::Block) -> Option<String> {
    if block.stmts.len() != 1 {
        return None;
    }
    let syn::Stmt::Expr(expr, _) = &block.stmts[0] else {
        return None;
    };
    let expr = if let syn::Expr::Return(ret) = expr {
        ret.expr.as_deref()?
    } else {
        expr
    };
    let (name, arguments) = match expr {
        syn::Expr::Call(call) => {
            let syn::Expr::Path(path) = call.func.as_ref() else {
                return None;
            };
            (path.path.segments.last()?.ident.to_string(), &call.args)
        }
        syn::Expr::MethodCall(call) => {
            let syn::Expr::Path(receiver) = call.receiver.as_ref() else {
                return None;
            };
            if !receiver.path.is_ident("self") {
                return None;
            }
            (call.method.to_string(), &call.args)
        }
        _ => return None,
    };
    if name == "new"
        || name.starts_with(char::is_uppercase)
        || !arguments.iter().all(direct_argument)
    {
        return None;
    }
    Some(name)
}
struct Scanner<'a> {
    path: &'a str,
    aliases: &'a BTreeMap<String, BTreeSet<String>>,
    test: bool,
    trait_impl: bool,
    owner: String,
    facts: &'a mut Facts,
}
impl Scanner<'_> {
    fn function(&mut self, sig: &syn::Signature, block: &syn::Block, vis: &Visibility) {
        let mut names = Names::default();
        names.visit_block(block);
        let ignored_parameters = sig
            .inputs
            .iter()
            .filter_map(|arg| match arg {
                syn::FnArg::Typed(arg) => match arg.pat.as_ref() {
                    syn::Pat::Ident(pat) if !names.names.contains(&pat.ident.to_string()) => {
                        Some(pat.ident.to_string())
                    }
                    syn::Pat::Wild(_) => Some("_".into()),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        self.facts.functions.push(Function {
            path: self.path.into(),
            name: sig.ident.to_string(),
            owner: self.owner.clone(),
            public: !matches!(vis, Visibility::Inherited),
            test_only: self.test,
            trait_impl: self.trait_impl,
            ignored_parameters,
            forwards_to: forwards(block),
        });
    }
    fn reference(&mut self, name: String) {
        if !["self", "Self", "crate", "super"].contains(&name.as_str()) {
            if let Some(originals) = self.aliases.get(&name) {
                for original in originals {
                    self.facts.references.push(Reference {
                        path: self.path.into(),
                        name: original.clone(),
                        test_only: self.test,
                    });
                }
            }
            self.facts.references.push(Reference {
                path: self.path.into(),
                name,
                test_only: self.test,
            });
        }
    }
}
impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        let previous = self.test;
        let attrs: &[syn::Attribute] = match item {
            Item::Fn(x) => &x.attrs,
            Item::Mod(x) => &x.attrs,
            Item::Impl(x) => &x.attrs,
            Item::Const(x) => &x.attrs,
            Item::Static(x) => &x.attrs,
            Item::Struct(x) => &x.attrs,
            Item::Enum(x) => &x.attrs,
            Item::Trait(x) => &x.attrs,
            Item::Type(x) => &x.attrs,
            _ => &[],
        };
        self.test |= test_attrs(attrs);
        let declared = match item {
            Item::Struct(x) => Some((&x.ident, &x.vis)),
            Item::Enum(x) => Some((&x.ident, &x.vis)),
            Item::Type(x) => Some((&x.ident, &x.vis)),
            Item::Trait(x) => Some((&x.ident, &x.vis)),
            Item::Const(x) => Some((&x.ident, &x.vis)),
            Item::Static(x) => Some((&x.ident, &x.vis)),
            _ => None,
        };
        if let Some((name, vis)) = declared {
            self.facts.declared_symbols.push(DeclaredSymbol {
                path: self.path.into(),
                name: name.to_string(),
                public: !matches!(vis, Visibility::Inherited),
                test_only: self.test,
            });
        }
        visit::visit_item(self, item);
        self.test = previous;
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        if !matches!(item.vis, Visibility::Inherited) {
            self.facts.reexport_files.insert(self.path.into());
        }
    } // Re-exports do not establish executable usage, but protect barrel modules.
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.function(&item.sig, &item.block, &item.vis);
        visit::visit_item_fn(self, item);
    }
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let previous_trait = self.trait_impl;
        let previous_owner = self.owner.clone();
        self.trait_impl = item.trait_.is_some();
        self.owner = if let syn::Type::Path(p) = item.self_ty.as_ref() {
            p.path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::")
        } else {
            "<impl>".into()
        };
        visit::visit_item_impl(self, item);
        self.trait_impl = previous_trait;
        self.owner = previous_owner;
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let previous = self.test;
        self.test |= test_attrs(&item.attrs);
        self.function(&item.sig, &item.block, &item.vis);
        visit::visit_impl_item_fn(self, item);
        self.test = previous;
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        for segment in &path.segments {
            self.reference(segment.ident.to_string());
        }
        visit::visit_path(self, path);
    }
    fn visit_expr_method_call(&mut self, item: &'ast syn::ExprMethodCall) {
        self.reference(item.method.to_string());
        visit::visit_expr_method_call(self, item);
    }
    fn visit_macro(&mut self, value: &'ast syn::Macro) {
        let mut names = BTreeSet::new();
        token_names(value.tokens.clone(), &mut names, format_macro(value));
        for name in names {
            self.reference(name);
        }
    }
}
fn path_attr(item: &syn::ItemMod) -> Option<String> {
    item.attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        if let Meta::NameValue(value) = &attr.meta {
            if let syn::Expr::Lit(value) = &value.value {
                if let syn::Lit::Str(value) = &value.lit {
                    return Some(value.value());
                }
            }
        }
        None
    })
}
fn walk(
    path: &Path,
    relocated: bool,
    test: bool,
    facts: &mut Facts,
) -> Result<(), Box<dyn std::error::Error>> {
    let name = path.to_string_lossy().into_owned();
    if !path.exists() {
        facts.missing_modules.insert(name);
        return Ok(());
    }
    let contexts = if test {
        &mut facts.test_files
    } else {
        &mut facts.production_files
    };
    if !contexts.insert(name.clone()) {
        return Ok(());
    }
    facts.registered.insert(name);
    let ast = syn::parse_file(&fs::read_to_string(path)?)?;
    let parent = path.parent().ok_or("module parent missing")?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("module stem missing")?;
    let dir = if relocated || matches!(stem, "lib" | "main" | "mod") {
        parent.to_owned()
    } else {
        parent.join(stem)
    };
    walk_items(&ast.items, parent, &dir, test, facts)
}
fn walk_items(
    items: &[Item],
    path_base: &Path,
    dir: &Path,
    test: bool,
    facts: &mut Facts,
) -> Result<(), Box<dyn std::error::Error>> {
    for item in items {
        let Item::Mod(item) = item else { continue };
        let relocated = path_attr(item);
        if let Some((_, contents)) = &item.content {
            let inline_dir =
                relocated.map_or_else(|| dir.join(item.ident.to_string()), |p| dir.join(p));
            walk_items(
                contents,
                &inline_dir,
                &inline_dir,
                test || test_attrs(&item.attrs),
                facts,
            )?;
        } else {
            let is_relocated = relocated.is_some();
            let file = if let Some(p) = relocated {
                path_base.join(p)
            } else {
                let flat = dir.join(format!("{}.rs", item.ident));
                if flat.exists() {
                    flat
                } else {
                    dir.join(item.ident.to_string()).join("mod.rs")
                }
            };
            // Normalize lexical '.' and '..' without requiring missing paths to exist.
            let mut normalized = PathBuf::new();
            for part in file.components() {
                match part {
                    std::path::Component::ParentDir => {
                        normalized.pop();
                    }
                    std::path::Component::CurDir => (),
                    _ => normalized.push(part),
                }
            }
            walk(
                &normalized,
                is_relocated,
                test || test_attrs(&item.attrs),
                facts,
            )?;
        }
    }
    Ok(())
}
struct Aliases<'a>(&'a mut BTreeMap<String, BTreeSet<String>>);
impl<'ast> Visit<'ast> for Aliases<'_> {
    fn visit_use_tree(&mut self, tree: &'ast syn::UseTree) {
        if let syn::UseTree::Rename(rename) = tree {
            self.0
                .entry(rename.rename.to_string())
                .or_default()
                .insert(rename.ident.to_string());
        }
        visit::visit_use_tree(self, tree);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ast_ignores_comments_tracks_aliases_macros_and_test_contexts() {
        let ast = syn::parse_file(
            r#"
            use crate::read as read_alias;
            // ghost();
            pub fn live(_ignored: bool, _used: bool) { read_alias(); assert!(_used); }
            #[cfg(test)] mod tests { fn check() { live(false, true); } }
            #[cfg(not(test))] fn production() { read_alias(); }
        "#,
        )
        .unwrap();
        let mut aliases = BTreeMap::new();
        Aliases(&mut aliases).visit_file(&ast);
        let mut facts = Facts::default();
        Scanner {
            path: "src/example.rs",
            aliases: &aliases,
            test: false,
            trait_impl: false,
            owner: String::new(),
            facts: &mut facts,
        }
        .visit_file(&ast);
        let live = facts.functions.iter().find(|f| f.name == "live").unwrap();
        assert_eq!(live.ignored_parameters, ["_ignored"]);
        assert!(!facts.references.iter().any(|r| r.name == "ghost"));
        assert!(facts
            .references
            .iter()
            .any(|r| r.name == "read" && !r.test_only));
        assert!(facts
            .references
            .iter()
            .any(|r| r.name == "live" && r.test_only));
        assert!(
            !facts
                .functions
                .iter()
                .find(|f| f.name == "production")
                .unwrap()
                .test_only
        );
    }
    #[test]
    fn captured_format_arguments_are_uses_but_escaped_braces_are_not() {
        let ast = syn::parse_file(
            r#"fn label(state: u32, value: u32) { vec![format!("{state} {value:02x} {{ghost}}")]; }"#,
        )
        .unwrap();
        let aliases = BTreeMap::new();
        let mut facts = Facts::default();
        Scanner {
            path: "src/example.rs",
            aliases: &aliases,
            test: false,
            trait_impl: false,
            owner: String::new(),
            facts: &mut facts,
        }
        .visit_file(&ast);
        assert!(facts.functions[0].ignored_parameters.is_empty());
        assert!(facts.references.iter().any(|r| r.name == "state"));
        assert!(!facts.references.iter().any(|r| r.name == "ghost"));
    }
    #[test]
    fn graph_traverses_all_platforms_relocated_inline_and_test_modules() {
        struct Temp(PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = Temp(
            std::env::temp_dir().join(format!("edpcli-redundancy-{}-{stamp}", std::process::id())),
        );
        fs::create_dir_all(dir.0.join("inline")).unwrap();
        fs::create_dir_all(dir.0.join("support")).unwrap();
        fs::write(
            dir.0.join("lib.rs"),
            r#"
            #[path="moved.rs"] mod moved;
            #[cfg(test)] #[path="support/check.rs"] mod check;
            #[cfg(target_os="other")] mod other;
            mod inline { #[path="nested.rs"] mod nested; }
        "#,
        )
        .unwrap();
        fs::write(dir.0.join("moved.rs"), "mod child;").unwrap();
        for name in [
            "child.rs",
            "other.rs",
            "inline/nested.rs",
            "support/check.rs",
        ] {
            fs::write(dir.0.join(name), "fn callback() {}").unwrap();
        }
        let mut facts = Facts::default();
        walk(&dir.0.join("lib.rs"), false, false, &mut facts).unwrap();
        assert_eq!(facts.registered.len(), 6);
        assert!(facts.missing_modules.is_empty());
        assert!(facts
            .production_files
            .contains(&dir.0.join("other.rs").to_string_lossy().into_owned()));
        assert!(facts
            .production_files
            .contains(&dir.0.join("child.rs").to_string_lossy().into_owned()));
        let check = dir
            .0
            .join("support/check.rs")
            .to_string_lossy()
            .into_owned();
        assert!(facts.test_files.contains(&check));
        assert!(!facts.production_files.contains(&check));
    }
    #[test]
    fn forwarding_does_not_confuse_transformations_or_field_accessors() {
        let thin: syn::ItemFn = syn::parse_quote!(
            fn thin(value: u32) {
                current(value)
            }
        );
        assert_eq!(forwards(&thin.block).as_deref(), Some("current"));
        let transformed: syn::ItemFn = syn::parse_quote!(
            fn transformed(value: u32) {
                current(value).map_err(|e| e.to_string())
            }
        );
        assert!(forwards(&transformed.block).is_none());
        let accessor: syn::ItemFn = syn::parse_quote!(
            fn accessor(&self) {
                self.value.as_ref()
            }
        );
        assert!(forwards(&accessor.block).is_none());
    }
    #[test]
    fn declarations_and_reexports_protect_current_module_contracts() {
        let ast = syn::parse_file("pub use crate::current::Live; pub struct LiveType; #[cfg(test)] pub type TestType = u64;").unwrap();
        let mut facts = Facts::default();
        let aliases = BTreeMap::new();
        Scanner {
            path: "src/facade.rs",
            aliases: &aliases,
            test: false,
            trait_impl: false,
            owner: String::new(),
            facts: &mut facts,
        }
        .visit_file(&ast);
        assert!(facts.reexport_files.contains("src/facade.rs"));
        assert!(facts
            .declared_symbols
            .iter()
            .any(|d| d.name == "LiveType" && d.public && !d.test_only));
        assert!(facts
            .declared_symbols
            .iter()
            .any(|d| d.name == "TestType" && d.test_only));
        assert!(!facts.references.iter().any(|r| r.name == "Live"));
    }
    #[test]
    fn cfg_any_test_and_feature_is_not_test_only() {
        let item: syn::ItemFn = syn::parse_quote!(
            #[cfg(any(test, feature = "demo"))]
            fn callback() {}
        );
        assert!(!test_attrs(&item.attrs));
        let item: syn::ItemFn = syn::parse_quote!(
            #[cfg(all(test, feature = "demo"))]
            fn callback() {}
        );
        assert!(test_attrs(&item.attrs));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let input: Input = serde_json::from_str(&input)?;
    let mut facts = Facts::default();
    for root in &input.roots {
        walk(
            Path::new(root),
            false,
            root.starts_with("tests/"),
            &mut facts,
        )?;
    }
    let mut aliases = BTreeMap::new();
    let sources = input
        .files
        .iter()
        .map(|file| Ok((file, syn::parse_file(&fs::read_to_string(file)?)?)))
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    for (_, ast) in &sources {
        Aliases(&mut aliases).visit_file(ast);
    }
    for (file, ast) in &sources {
        let test = file.starts_with("tests/")
            || (facts.test_files.contains(*file) && !facts.production_files.contains(*file));
        Scanner {
            path: file,
            aliases: &aliases,
            test,
            trait_impl: false,
            owner: String::new(),
            facts: &mut facts,
        }
        .visit_file(ast);
    }
    println!("{}", serde_json::to_string(&facts)?);
    Ok(())
}
