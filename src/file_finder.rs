use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn find_latest_live_log(log_dir: &Path) -> Result<PathBuf, String> {
    let entries = fs::read_dir(log_dir)
        .map_err(|e| format!("Failed to read directory '{}': {}", log_dir.display(), e))?;

    let mut live_log_files: Vec<PathBuf> = entries
        .filter_map(|entry_result| {
            entry_result.ok().and_then(|entry| {
                let path = entry.path();
                if !path.is_file() {
                    return None;
                }

                let file_name = path.file_name()?.to_str()?;
                if !file_name.ends_with(".log") {
                    return None;
                }

                let base_name = file_name.strip_suffix(".log").unwrap();
                if let Some(last_dot_pos) = base_name.rfind('.') {
                    let suffix = &base_name[last_dot_pos + 1..];
                    if suffix.parse::<u32>().is_ok() {
                        return None; // Exclude rotated logs like `file.1.log`
                    }
                }
                Some(path)
            })
        })
        .collect();

    if live_log_files.is_empty() {
        return Err("No live log files found in the directory.".to_string());
    }

    live_log_files.sort();
    Ok(live_log_files.pop().unwrap())
}
