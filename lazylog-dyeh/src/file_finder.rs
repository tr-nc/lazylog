use std::{
    fs,
    path::{Path, PathBuf},
};

fn collect_matching_files(
    base_path: &Path,
    matcher: &dyn Fn(&Path) -> bool,
    files: &mut Vec<PathBuf>,
) {
    if let Ok(entries) = fs::read_dir(base_path) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                collect_matching_files(&path, matcher, files);
                continue;
            }

            if path.is_file() && matcher(&path) {
                files.push(path);
            }
        }
    }
}

fn find_latest_file(files: Vec<PathBuf>, empty_message: &str) -> Result<PathBuf, String> {
    let mut dated_files: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();

    for file_path in files {
        if let Ok(metadata) = fs::metadata(&file_path)
            && let Ok(modified) = metadata.modified()
        {
            dated_files.push((file_path, modified));
        }
    }

    if dated_files.is_empty() {
        return Err(empty_message.to_string());
    }

    dated_files.sort_by_key(|(_, modified)| *modified);
    Ok(dated_files.pop().unwrap().0)
}

/// recursively find all 'previewLog' directories under the given path (DYEH-specific)
pub fn find_preview_log_dirs(base_path: &Path) -> Vec<PathBuf> {
    let mut preview_log_dirs = Vec::new();

    if let Ok(entries) = fs::read_dir(base_path) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                // check if this directory is named 'previewLog'
                if let Some(dir_name) = path.file_name()
                    && dir_name == "previewLog"
                {
                    preview_log_dirs.push(path.clone());
                }

                // recursively search in subdirectories
                let mut subdirs = find_preview_log_dirs(&path);
                preview_log_dirs.append(&mut subdirs);
            }
        }
    }

    preview_log_dirs
}

/// find the latest live log file from DYEH previewLog directories
pub fn find_latest_live_log(preview_log_dirs: Vec<PathBuf>) -> Result<PathBuf, String> {
    if preview_log_dirs.is_empty() {
        return Err("No previewLog directories found.".to_string());
    }

    let mut all_live_log_files = Vec::new();

    // search for live log files in all preview log directories
    for log_dir in &preview_log_dirs {
        let entries = fs::read_dir(log_dir)
            .map_err(|e| format!("Failed to read directory '{}': {}", log_dir.display(), e))?;

        let live_log_files: Vec<PathBuf> = entries
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
                            return None; // exclude rotated logs like `file.1.log`
                        }
                    }
                    Some(path)
                })
            })
            .collect();

        all_live_log_files.extend(live_log_files);
    }

    find_latest_file(
        all_live_log_files,
        "No live log files found in any previewLog directories.",
    )
}

/// find the latest editor log file under a DYEH Logs/Log tree
pub fn find_latest_editor_log(base_path: &Path) -> Result<PathBuf, String> {
    let mut editor_log_files = Vec::new();
    collect_matching_files(
        base_path,
        &|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("-editor.log"))
        },
        &mut editor_log_files,
    );

    find_latest_file(editor_log_files, "No editor log files found.")
}
