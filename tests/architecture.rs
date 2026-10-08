//! The simulation (`src/game`) must stay pure: no files, clocks, terminals, windows or audio.
//! That keeps it deterministic and testable without any system libraries.

#[test]
fn game_module_has_no_io() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/game");
    let banned = ["std::fs", "std::time", "Instant", "SystemTime", "crossterm", "minifb", "kira", "std::io", "HashMap", "thread_rng"];
    let mut problems = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        // Tests may read room files from disk.
        let code = text.split("#[cfg(test)]").next().unwrap_or("");
        for (n, line) in code.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for b in banned {
                if line.contains(b) {
                    problems.push(format!("{}:{}: uses `{b}`", path.display(), n + 1));
                }
            }
        }
    }
    assert!(problems.is_empty(), "game/ must stay pure:\n{}", problems.join("\n"));
}
