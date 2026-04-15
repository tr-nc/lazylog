use crate::{file_finder, metadata};
use anyhow::Result;
use lazy_static::lazy_static;
use lazylog_framework::provider::LogProvider;
use memmap2::MmapOptions;
use regex::Regex;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

lazy_static! {
    static ref EDITOR_LOG_HEADER_RE: Regex =
        Regex::new(r"^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}\] \[[^\]]+\]").unwrap();
}

/// log provider for DYEH logs (file-based)
pub struct DyehLogProvider {
    log_dir_path: PathBuf,
    log_file_path: PathBuf,
    source: DyehLogSource,
    last_len: u64,
    prev_meta: Option<metadata::MetaSnap>,
}

#[derive(Clone, Copy)]
enum DyehLogSource {
    Preview,
    Editor,
}

impl DyehLogProvider {
    pub fn new(log_dir_path: PathBuf) -> Self {
        Self::new_with_source(log_dir_path, DyehLogSource::Preview)
    }

    pub fn new_editor(log_dir_path: PathBuf) -> Self {
        Self::new_with_source(log_dir_path, DyehLogSource::Editor)
    }

    fn new_with_source(log_dir_path: PathBuf, source: DyehLogSource) -> Self {
        let log_file_path = match Self::find_latest_log_file(&log_dir_path, source) {
            Ok(path) => {
                log::debug!(
                    "DyehLogProvider: Found initial log file: {}",
                    path.display()
                );
                path
            }
            Err(e) => {
                log::debug!("DyehLogProvider: No log files found initially: {}", e);
                log_dir_path.join("__no_log_file_yet__.log")
            }
        };

        Self {
            log_dir_path,
            log_file_path,
            source,
            last_len: 0,
            prev_meta: None,
        }
    }

    fn find_latest_log_file(log_dir_path: &Path, source: DyehLogSource) -> Result<PathBuf> {
        let logs_path = log_dir_path.join("Logs");
        let log_path = log_dir_path.join("Log");

        match source {
            DyehLogSource::Preview => {
                let mut preview_log_dirs = Vec::new();

                if logs_path.exists() {
                    preview_log_dirs.extend(file_finder::find_preview_log_dirs(&logs_path));
                }

                if log_path.exists() {
                    preview_log_dirs.extend(file_finder::find_preview_log_dirs(&log_path));
                }

                file_finder::find_latest_live_log(preview_log_dirs).map_err(anyhow::Error::msg)
            }
            DyehLogSource::Editor => {
                let mut editor_log_files = Vec::new();

                if logs_path.exists() {
                    if let Ok(path) = file_finder::find_latest_editor_log(&logs_path) {
                        editor_log_files.push(path);
                    }
                }

                if log_path.exists() {
                    if let Ok(path) = file_finder::find_latest_editor_log(&log_path) {
                        editor_log_files.push(path);
                    }
                }

                if editor_log_files.is_empty() {
                    anyhow::bail!("No editor log files found.");
                }

                let mut latest_file = editor_log_files[0].clone();
                let mut latest_modified = std::fs::metadata(&latest_file)?.modified()?;

                for file_path in editor_log_files.into_iter().skip(1) {
                    let modified = std::fs::metadata(&file_path)?.modified()?;
                    if modified > latest_modified {
                        latest_modified = modified;
                        latest_file = file_path;
                    }
                }

                Ok(latest_file)
            }
        }
    }

    fn check_for_newer_log_file(&self) -> Result<Option<PathBuf>> {
        match Self::find_latest_log_file(&self.log_dir_path, self.source) {
            Ok(latest_file_path) => {
                if !self.log_file_path.exists() {
                    log::debug!(
                        "DyehLogProvider: Found first log file: {}",
                        latest_file_path.display()
                    );
                    Ok(Some(latest_file_path))
                } else if latest_file_path != self.log_file_path {
                    log::debug!(
                        "DyehLogProvider: Found newer log file: {} (current: {})",
                        latest_file_path.display(),
                        self.log_file_path.display()
                    );
                    Ok(Some(latest_file_path))
                } else {
                    Ok(None)
                }
            }
            Err(e) => {
                log::debug!("DyehLogProvider: No log files found yet: {}", e);
                Ok(None)
            }
        }
    }

    fn switch_to_log_file(&mut self, new_file_path: PathBuf) {
        log::debug!(
            "DyehLogProvider: Switching from {} to {}",
            self.log_file_path.display(),
            new_file_path.display()
        );
        self.log_file_path = new_file_path;
        self.last_len = 0;
        self.prev_meta = None;
    }

    fn read_delta(file_path: &Path, prev_len: u64, cur_len: u64) -> Result<Vec<String>> {
        let file = File::open(file_path)?;
        let mmap = unsafe { MmapOptions::new().len(cur_len as usize).map(&file)? };

        let start = (prev_len as usize).min(mmap.len());
        let end = (cur_len as usize).min(mmap.len());
        let delta_bytes = &mmap[start..end];

        if delta_bytes.is_empty() {
            return Ok(Vec::new());
        }

        let delta_str = String::from_utf8_lossy(delta_bytes);

        let log_blocks = Self::split_log_blocks(file_path, &delta_str);

        Ok(log_blocks)
    }

    fn split_log_blocks(file_path: &Path, text: &str) -> Vec<String> {
        match Self::source_from_path(file_path) {
            DyehLogSource::Preview => Self::split_preview_blocks(text),
            DyehLogSource::Editor => Self::split_editor_blocks(text),
        }
    }

    fn source_from_path(file_path: &Path) -> DyehLogSource {
        let is_editor = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("-editor.log"));

        if is_editor {
            DyehLogSource::Editor
        } else {
            DyehLogSource::Preview
        }
    }

    fn split_preview_blocks(text: &str) -> Vec<String> {
        lazy_static! {
            static ref MARKER_RE: Regex =
                Regex::new(r"## \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}").unwrap();
        }

        let mut blocks = Vec::new();
        let mut starts: Vec<usize> = MARKER_RE.find_iter(text).map(|m| m.start()).collect();

        if starts.is_empty() {
            return blocks;
        }

        // add sentinel for the last block
        starts.push(text.len());

        // extract each block from one ## to the next ##
        for window in starts.windows(2) {
            if let [start, end] = *window {
                let block = text[start..end].trim().to_string();
                if !block.is_empty() {
                    blocks.push(block);
                }
            }
        }

        blocks
    }

    fn split_editor_blocks(text: &str) -> Vec<String> {
        let mut blocks = Vec::new();
        let mut current_block = String::new();

        for line in text.lines() {
            let is_new_entry = EDITOR_LOG_HEADER_RE.is_match(line);

            if is_new_entry && !current_block.is_empty() {
                blocks.push(current_block.trim().to_string());
                current_block.clear();
            }

            if !current_block.is_empty() {
                current_block.push('\n');
            }
            current_block.push_str(line);
        }

        if !current_block.is_empty() {
            blocks.push(current_block.trim().to_string());
        }

        blocks
    }
}

impl LogProvider for DyehLogProvider {
    fn start(&mut self) -> Result<()> {
        log::debug!("DyehLogProvider: Starting");
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        log::debug!("DyehLogProvider: Stopping");
        Ok(())
    }

    fn poll_logs(&mut self) -> Result<Vec<String>> {
        // check for newer log file
        if let Ok(Some(newer_file)) = self.check_for_newer_log_file() {
            self.switch_to_log_file(newer_file);
        }

        if !self.log_file_path.exists() {
            return Ok(Vec::new());
        }

        let current_meta = match metadata::stat_path(&self.log_file_path) {
            Ok(m) => m,
            Err(_) => return Ok(Vec::new()),
        };

        if !metadata::has_changed(&self.prev_meta, &current_meta) {
            return Ok(Vec::new());
        }

        // handle file truncation
        if current_meta.len < self.last_len {
            self.last_len = 0;
        }

        let mut log_blocks = Vec::new();
        if current_meta.len > self.last_len {
            match Self::read_delta(&self.log_file_path, self.last_len, current_meta.len) {
                Ok(blocks) => {
                    log::debug!("DyehLogProvider: Read {} new log blocks", blocks.len());
                    log_blocks = blocks;
                }
                Err(e) => {
                    log::debug!("DyehLogProvider: Error reading delta: {}", e);
                }
            }
            self.last_len = current_meta.len;
        }

        self.prev_meta = Some(current_meta);
        Ok(log_blocks)
    }
}
