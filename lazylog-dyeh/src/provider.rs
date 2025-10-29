use crate::{file_finder, metadata};
use anyhow::Result;
use lazylog_framework::provider::{LogItem, LogProvider};
use lazylog_parser::process_delta;
use memmap2::MmapOptions;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

/// log provider for DYEH logs (file-based)
pub struct DyehLogProvider {
    log_dir_path: PathBuf,
    log_file_path: PathBuf,
    last_len: u64,
    prev_meta: Option<metadata::MetaSnap>,
}

impl DyehLogProvider {
    pub fn new(log_dir_path: PathBuf) -> Self {
        // DYEH 540 adaptation: check both "Logs" and "Log" subdirectories
        let mut preview_log_dirs = Vec::new();

        let logs_path = log_dir_path.join("Logs");
        if logs_path.exists() {
            preview_log_dirs.extend(file_finder::find_preview_log_dirs(&logs_path));
        }

        let log_path = log_dir_path.join("Log");
        if log_path.exists() {
            preview_log_dirs.extend(file_finder::find_preview_log_dirs(&log_path));
        }

        let log_file_path = match file_finder::find_latest_live_log(preview_log_dirs) {
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
            last_len: 0,
            prev_meta: None,
        }
    }

    fn check_for_newer_log_file(&self) -> Result<Option<PathBuf>> {
        // DYEH 540 adaptation: check both "Logs" and "Log" subdirectories
        let mut preview_log_dirs = Vec::new();

        let logs_path = self.log_dir_path.join("Logs");
        if logs_path.exists() {
            preview_log_dirs.extend(file_finder::find_preview_log_dirs(&logs_path));
        }

        let log_path = self.log_dir_path.join("Log");
        if log_path.exists() {
            preview_log_dirs.extend(file_finder::find_preview_log_dirs(&log_path));
        }

        match file_finder::find_latest_live_log(preview_log_dirs) {
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

    fn read_delta(file_path: &Path, prev_len: u64, cur_len: u64) -> Result<Vec<LogItem>> {
        let file = File::open(file_path)?;
        let mmap = unsafe { MmapOptions::new().len(cur_len as usize).map(&file)? };

        let start = (prev_len as usize).min(mmap.len());
        let end = (cur_len as usize).min(mmap.len());
        let delta_bytes = &mmap[start..end];

        if delta_bytes.is_empty() {
            return Ok(Vec::new());
        }

        let delta_str = String::from_utf8_lossy(delta_bytes);
        let log_items = process_delta(&delta_str);

        Ok(log_items)
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

    fn poll_logs(&mut self) -> Result<Vec<LogItem>> {
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

        let mut new_items = Vec::new();
        if current_meta.len > self.last_len {
            match Self::read_delta(&self.log_file_path, self.last_len, current_meta.len) {
                Ok(items) => {
                    log::debug!("DyehLogProvider: Read {} new log items", items.len());
                    new_items = items;
                }
                Err(e) => {
                    log::debug!("DyehLogProvider: Error reading delta: {}", e);
                }
            }
            self.last_len = current_meta.len;
        }

        self.prev_meta = Some(current_meta);
        Ok(new_items)
    }
}
