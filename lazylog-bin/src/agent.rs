use lazylog_framework::provider::{LogParser, LogProvider};
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    flag, low_level,
};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) const DEFAULT_PREVIEW_LINES: usize = 500;
pub(crate) const DEFAULT_PREVIEW_BYTES: usize = 64 * 1024;

pub(crate) struct AgentOptions {
    pub(crate) capture_file: Option<PathBuf>,
    pub(crate) preview_lines: usize,
    pub(crate) preview_bytes: usize,
    pub(crate) duration: Option<Duration>,
}

impl Default for AgentOptions {
    fn default() -> Self {
        Self {
            capture_file: None,
            preview_lines: DEFAULT_PREVIEW_LINES,
            preview_bytes: DEFAULT_PREVIEW_BYTES,
            duration: None,
        }
    }
}

enum PreviewEvent {
    LimitReached,
    WriteFailed(io::Error),
}

struct AgentOutput<C, P> {
    capture: C,
    preview: P,
    max_preview_lines: usize,
    max_preview_bytes: usize,
    preview_lines: usize,
    preview_bytes: usize,
    captured_items: usize,
    captured_lines: usize,
    captured_bytes: usize,
    preview_disabled: bool,
}

impl<C: Write, P: Write> AgentOutput<C, P> {
    fn new(capture: C, preview: P, max_preview_lines: usize, max_preview_bytes: usize) -> Self {
        Self {
            capture,
            preview,
            max_preview_lines,
            max_preview_bytes,
            preview_lines: 0,
            preview_bytes: 0,
            captured_items: 0,
            captured_lines: 0,
            captured_bytes: 0,
            preview_disabled: false,
        }
    }

    fn write_item(&mut self, content: &str) -> io::Result<Option<PreviewEvent>> {
        let item_lines = physical_line_count(content);
        let item_bytes = content.len().saturating_add(1);

        self.capture.write_all(content.as_bytes())?;
        self.capture.write_all(b"\n")?;
        self.captured_items = self.captured_items.saturating_add(1);
        self.captured_lines = self.captured_lines.saturating_add(item_lines);
        self.captured_bytes = self.captured_bytes.saturating_add(item_bytes);

        if self.preview_disabled {
            return Ok(None);
        }

        let exceeds_lines = self.preview_lines.saturating_add(item_lines) > self.max_preview_lines;
        let exceeds_bytes = self.preview_bytes.saturating_add(item_bytes) > self.max_preview_bytes;
        if exceeds_lines || exceeds_bytes {
            self.preview_disabled = true;
            return Ok(Some(PreviewEvent::LimitReached));
        }

        if let Err(error) = self
            .preview
            .write_all(content.as_bytes())
            .and_then(|_| self.preview.write_all(b"\n"))
            .and_then(|_| self.preview.flush())
        {
            self.preview_disabled = true;
            return Ok(Some(PreviewEvent::WriteFailed(error)));
        }

        self.preview_lines += item_lines;
        self.preview_bytes += item_bytes;
        Ok(None)
    }

    fn flush_capture(&mut self) -> io::Result<()> {
        self.capture.flush()
    }

    fn finish(&mut self) -> io::Result<()> {
        self.capture.flush()?;
        if !self.preview_disabled {
            let _ = self.preview.flush();
        }
        Ok(())
    }
}

fn physical_line_count(content: &str) -> usize {
    content
        .as_bytes()
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        .saturating_add(1)
}

fn absolute_path(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn open_new_capture(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path)
}

fn create_capture_file(requested_path: Option<&Path>) -> io::Result<(PathBuf, File)> {
    if let Some(path) = requested_path {
        let path = absolute_path(path)?;
        let file = open_new_capture(&path)?;
        return Ok((path, file));
    }

    let capture_dir = dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("lazylog")
        .join("captures");
    fs::create_dir_all(&capture_dir)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let process_id = std::process::id();

    for suffix in 0..1000 {
        let suffix = if suffix == 0 {
            String::new()
        } else {
            format!("-{suffix}")
        };
        let path = capture_dir.join(format!("lazylog-{timestamp}-{process_id}{suffix}.log"));
        match open_new_capture(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique lazylog capture file",
    ))
}

struct SignalRegistrations(Vec<signal_hook::SigId>);

impl Drop for SignalRegistrations {
    fn drop(&mut self) {
        for signal_id in self.0.drain(..) {
            low_level::unregister(signal_id);
        }
    }
}

fn install_stop_signals() -> io::Result<(Arc<AtomicBool>, SignalRegistrations)> {
    let stop = Arc::new(AtomicBool::new(false));
    let mut registrations = SignalRegistrations(Vec::new());

    for signal in [SIGINT, SIGTERM] {
        match flag::register(signal, stop.clone()) {
            Ok(signal_id) => registrations.0.push(signal_id),
            Err(error) => return Err(error),
        }
    }

    Ok((stop, registrations))
}

fn capture_batch<C: Write, P: Write>(
    raw_logs: Vec<String>,
    parser: &Arc<dyn LogParser>,
    initial_filter: Option<&str>,
    output: &mut AgentOutput<C, P>,
    capture_path: &Path,
) -> io::Result<()> {
    for raw_log in raw_logs {
        let Some(item) = parser.parse(&raw_log) else {
            continue;
        };
        if !super::matches_filter(parser, &item, initial_filter) {
            continue;
        }

        match output.write_item(&item.raw_content)? {
            Some(PreviewEvent::LimitReached) => eprintln!(
                "[lazylog] preview limit reached after {} lines / {} bytes; capture continues at {}",
                output.preview_lines,
                output.preview_bytes,
                capture_path.display()
            ),
            Some(PreviewEvent::WriteFailed(error)) => eprintln!(
                "[lazylog] stdout preview disabled after write error: {error}; capture continues at {}",
                capture_path.display()
            ),
            None => {}
        }
    }

    output.flush_capture()
}

fn remember_first_error(slot: &mut Option<io::Error>, result: io::Result<()>) {
    if slot.is_none()
        && let Err(error) = result
    {
        *slot = Some(error);
    }
}

pub(crate) fn run_agent<P>(
    mut provider: P,
    parser: Arc<dyn LogParser>,
    initial_filter: Option<&str>,
    poll_interval: Duration,
    options: &AgentOptions,
) -> io::Result<()>
where
    P: LogProvider,
{
    let (capture_path, capture_file) = create_capture_file(options.capture_file.as_deref())?;
    let (stop, _signal_registrations) = install_stop_signals()?;
    let stdout = io::stdout();
    let mut output = AgentOutput::new(
        BufWriter::new(capture_file),
        stdout.lock(),
        options.preview_lines,
        options.preview_bytes,
    );

    eprintln!("[lazylog] complete capture: {}", capture_path.display());
    provider.start().map_err(io::Error::other)?;

    let started_at = Instant::now();
    let mut first_error = None;

    while !stop.load(Ordering::Relaxed)
        && options
            .duration
            .is_none_or(|duration| started_at.elapsed() < duration)
    {
        match provider.poll_logs() {
            Ok(raw_logs) => {
                if let Err(error) = capture_batch(
                    raw_logs,
                    &parser,
                    initial_filter,
                    &mut output,
                    &capture_path,
                ) {
                    first_error = Some(error);
                    break;
                }
            }
            Err(error) => eprintln!("Provider poll error: {error}"),
        }

        let sleep_for = options
            .duration
            .map(|duration| {
                duration
                    .saturating_sub(started_at.elapsed())
                    .min(poll_interval)
            })
            .unwrap_or(poll_interval);
        if !sleep_for.is_zero() {
            thread::sleep(sleep_for);
        }
    }

    remember_first_error(&mut first_error, provider.stop().map_err(io::Error::other));

    if first_error.is_none() {
        match provider.poll_logs() {
            Ok(raw_logs) => remember_first_error(
                &mut first_error,
                capture_batch(
                    raw_logs,
                    &parser,
                    initial_filter,
                    &mut output,
                    &capture_path,
                ),
            ),
            Err(error) => eprintln!("Provider final poll error: {error}"),
        }
    }

    remember_first_error(&mut first_error, output.finish());
    eprintln!(
        "[lazylog] capture complete: {} ({} items, {} lines, {} bytes)",
        capture_path.display(),
        output.captured_items,
        output.captured_lines,
        output.captured_bytes
    );

    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_lines_include_the_record_terminator() {
        assert_eq!(physical_line_count(""), 1);
        assert_eq!(physical_line_count("one"), 1);
        assert_eq!(physical_line_count("one\ntwo"), 2);
        assert_eq!(physical_line_count("one\n"), 2);
    }

    #[test]
    fn preview_stops_at_line_limit_but_capture_continues() {
        let mut output = AgentOutput::new(Vec::new(), Vec::new(), 2, usize::MAX);

        assert!(output.write_item("one\ntwo").unwrap().is_none());
        assert!(matches!(
            output.write_item("three").unwrap(),
            Some(PreviewEvent::LimitReached)
        ));
        assert!(output.write_item("four").unwrap().is_none());
        output.finish().unwrap();

        assert_eq!(output.capture, b"one\ntwo\nthree\nfour\n");
        assert_eq!(output.preview, b"one\ntwo\n");
        assert_eq!(output.captured_items, 3);
        assert_eq!(output.captured_lines, 4);
    }

    #[test]
    fn preview_stops_before_exceeding_byte_limit() {
        let mut output = AgentOutput::new(Vec::new(), Vec::new(), usize::MAX, 4);

        assert!(output.write_item("abc").unwrap().is_none());
        assert!(matches!(
            output.write_item("d").unwrap(),
            Some(PreviewEvent::LimitReached)
        ));

        assert_eq!(output.capture, b"abc\nd\n");
        assert_eq!(output.preview, b"abc\n");
    }
}
