use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=src/rules/");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let rules_dir = Path::new(&manifest_dir).join("src/rules");

    let mut modules: Vec<String> = fs::read_dir(&rules_dir)
        .expect("src/rules/ directory not found")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    modules.sort();

    let content: String = modules
        .iter()
        .map(|m| {
            let rule_path = rules_dir.join(m).join("mod.rs");
            format!("#[path = {:?}]\npub(crate) mod {};\n", rule_path, m)
        })
        .collect();

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("rule_modules.rs");
    fs::write(dest, content).unwrap();
}
