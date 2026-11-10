use super::App;
use anyhow::{Context, Result};
use std::{
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::Duration,
};

impl App {
    pub(super) fn export_displayed_logs(&mut self) {
        let message = if self.displaying_logs.indices.is_empty() {
            "No displayed logs to export".to_string()
        } else {
            let result = dirs::home_dir()
                .context("Could not locate home directory")
                .and_then(|home| self.export_displayed_logs_to(&home.join("Downloads")));
            match result {
                Ok(path) => format!(
                    "Exported {} logs: {}",
                    self.displaying_logs.len(),
                    path.display()
                ),
                Err(error) => {
                    log::error!("Failed to export logs: {error:#}");
                    format!("Export failed: {error:#}")
                }
            }
        };
        // Allow enough time to read the filename, unlike brief copy notifications.
        self.set_display_event(message, Duration::from_secs(15), None);
    }

    fn export_displayed_logs_to(&self, directory: &Path) -> Result<PathBuf> {
        fs::create_dir_all(directory)
            .with_context(|| format!("Could not create {}", directory.display()))?;
        let (path, file) = create_export_file(directory, || {
            uuid::Uuid::new_v4().simple().to_string()[..6].to_string()
        })?;
        let result = (|| -> std::io::Result<()> {
            let mut writer = BufWriter::new(file);
            // Match copy-all's parser formatting and order without building one giant string.
            for (position, &raw_index) in self.displaying_logs.indices.iter().enumerate() {
                if position > 0 {
                    writer.write_all(b"\n\n")?;
                }
                writer.write_all(
                    self.parser
                        .make_yank_content(&self.raw_logs[raw_index])
                        .as_bytes(),
                )?;
            }
            writer.flush()
        })();
        if let Err(error) = result {
            // Do not leave a truncated export that looks successful.
            let _ = fs::remove_file(&path);
            return Err(error).with_context(|| format!("Could not write {}", path.display()));
        }
        Ok(path)
    }
}

fn create_export_file(
    directory: &Path,
    mut next_suffix: impl FnMut() -> String,
) -> Result<(PathBuf, fs::File)> {
    loop {
        let path = directory.join(format!("lazylog-{}.log", next_suffix()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("Could not create {}", path.display()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AppDesc, LogItem,
        app::events::tests::{NoopParser, NoopProvider},
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::sync::Arc;

    #[test]
    fn export_preserves_all_twenty_thousand_entries_and_does_not_overwrite() {
        let directory = std::env::temp_dir().join(format!("lazylog-test-{}", uuid::Uuid::new_v4()));
        let mut app = App::new(NoopProvider, AppDesc::new(Arc::new(NoopParser)));
        app.raw_logs = (0..20_000)
            .map(|i| {
                LogItem::new(
                    format!("preview {i}"),
                    format!("raw {i}: 中文\nsecond line"),
                )
            })
            .collect();
        app.displaying_logs.indices = (0..20_000).collect();
        let expected = app
            .raw_logs
            .iter()
            .map(|item| app.parser.make_yank_content(item))
            .collect::<Vec<_>>()
            .join("\n\n");

        let first = app.export_displayed_logs_to(&directory).unwrap();
        let second = app.export_displayed_logs_to(&directory).unwrap();
        assert_ne!(first, second);
        assert_eq!(first.extension().unwrap(), "log");
        let filename = first.file_name().unwrap().to_str().unwrap();
        let suffix = filename
            .strip_prefix("lazylog-")
            .unwrap()
            .strip_suffix(".log")
            .unwrap();
        assert_eq!(suffix.len(), 6);
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(fs::read_to_string(&first).unwrap(), expected);
        assert_eq!(fs::read_to_string(&second).unwrap(), expected);

        // Only currently displayed indices, in displayed order, are exported.
        app.displaying_logs.indices = vec![19_999, 2];
        let filtered = app.export_displayed_logs_to(&directory).unwrap();
        assert_eq!(
            fs::read_to_string(filtered).unwrap(),
            format!(
                "{}\n\n{}",
                app.parser.make_yank_content(&app.raw_logs[19_999]),
                app.parser.make_yank_content(&app.raw_logs[2])
            )
        );
        app.cleanup();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn empty_export_key_reports_no_logs_and_respects_input_modes() {
        let mut app = App::new(NoopProvider, AppDesc::new(Arc::new(NoopParser)));
        let key = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE);
        app.show_help_popup = true;
        app.handle_key(key).unwrap();
        assert!(app.display_event.is_none());
        app.show_help_popup = false;
        app.filter_input = "/".to_string();
        app.filter_focused = true;
        app.handle_key(key).unwrap();
        assert_eq!(app.filter_input, "/e");
        assert!(app.display_event.is_none());
        app.filter_focused = false;
        app.handle_key(key).unwrap();
        assert_eq!(
            app.display_event.as_ref().unwrap().text,
            "No displayed logs to export"
        );
        app.cleanup();
    }

    #[test]
    fn filename_collision_retries_without_overwriting_existing_export() {
        let directory = std::env::temp_dir().join(format!("lazylog-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let existing = directory.join("lazylog-abcdef.log");
        fs::write(&existing, "existing export").unwrap();
        let mut suffixes = ["abcdef", "123456"].into_iter();
        let (path, file) =
            create_export_file(&directory, || suffixes.next().unwrap().to_string()).unwrap();
        assert_eq!(path.file_name().unwrap(), "lazylog-123456.log");
        assert_eq!(fs::read_to_string(existing).unwrap(), "existing export");
        drop(file);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn invalid_destination_returns_an_error_without_overwriting() {
        let path = std::env::temp_dir().join(format!("lazylog-test-{}", uuid::Uuid::new_v4()));
        fs::write(&path, "existing file").unwrap();
        let mut app = App::new(NoopProvider, AppDesc::new(Arc::new(NoopParser)));
        assert!(app.export_displayed_logs_to(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "existing file");
        app.cleanup();
        fs::remove_file(path).unwrap();
    }
}
