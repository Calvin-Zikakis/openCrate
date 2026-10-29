use std::fs;
use std::path::{Path, PathBuf};

/// Every `.rs` file under `folder`, test files left out.
fn sources(folder: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "target" && name != "tests" {
                sources(&path, found);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") && !name.ends_with("tests.rs") {
            found.push(path);
        }
    }
}

/// The part of a file before its inline test module (`#[cfg(test)]` then `mod name {`); a
/// one-line `mod name;` declaration of a test file does not count.
fn shipped(text: &str) -> &str {
    let lines: Vec<&str> = text.lines().collect();
    let mut offset = 0;
    for (index, line) in lines.iter().enumerate() {
        // Other attributes may sit between `#[cfg(test)]` and the module.
        let next = lines[index + 1..]
            .iter()
            .map(|next| next.trim())
            .find(|next| !next.starts_with("#["))
            .unwrap_or("");
        if line.trim() == "#[cfg(test)]" && next.starts_with("mod ") && next.ends_with('{') {
            return &text[..offset];
        }
        offset += line.len() + 1;
    }
    text
}

#[test]
fn only_an_inline_test_module_counts_as_test_code() {
    let text =
        "#[cfg(test)]\nmod process_tests;\nfn a() {}\n#[cfg(test)]\n#[allow(x)]\nmod tests {\n}\n";
    assert_eq!(
        shipped(text),
        "#[cfg(test)]\nmod process_tests;\nfn a() {}\n"
    );
}

#[test]
fn programs_are_started_only_through_the_helper_that_hides_their_windows() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    sources(&root.join("crates"), &mut files);
    sources(&root.join("src-tauri/src"), &mut files);
    assert!(files.len() > 20, "the workspace was found");
    let mut direct = Vec::new();
    for file in files {
        if file.ends_with("oc-convert/src/process.rs") {
            continue;
        }
        let text = fs::read_to_string(&file).unwrap();
        if shipped(&text).contains("Command::new(") {
            direct.push(file.display().to_string());
        }
    }
    assert_eq!(
        direct,
        Vec::<String>::new(),
        "start programs with oc_convert::process::command, which keeps Windows from opening a \
         console window for each one"
    );
}
