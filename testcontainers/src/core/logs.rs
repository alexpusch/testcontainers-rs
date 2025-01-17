use conquer_once::Lazy;
#[cfg(feature = "experimental")]
use futures::{stream::BoxStream, StreamExt};
use std::{
    fmt, io,
    io::{BufRead, BufReader, Read},
    num::NonZeroU8,
    path::{Path, PathBuf},
};
use time::{
    format_description::well_known::{self, iso8601::TimePrecision},
    OffsetDateTime,
};

static LOGS_DUMP_DIR_PATH: Lazy<PathBuf> = Lazy::new(|| {
    PathBuf::from("testcontainers").join(
        // now date in iso8601 format, with 2 digits precision
        OffsetDateTime::now_utc()
            .format(
                &well_known::Iso8601::<
                    {
                        well_known::iso8601::Config::DEFAULT
                            .set_time_precision(TimePrecision::Second {
                                decimal_digits: NonZeroU8::new(2),
                            })
                            .encode()
                    },
                >,
            )
            .unwrap_or("".into()),
    )
});

#[cfg(feature = "experimental")]
pub(crate) struct LogStreamAsync<'d> {
    inner: BoxStream<'d, Result<String, std::io::Error>>,
}

#[cfg(feature = "experimental")]
impl<'d> fmt::Debug for LogStreamAsync<'d> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogStreamAsync").finish()
    }
}

#[cfg(feature = "experimental")]
impl<'d> LogStreamAsync<'d> {
    pub fn new(stream: BoxStream<'d, Result<String, std::io::Error>>) -> Self {
        Self { inner: stream }
    }

    pub async fn wait_for_message(mut self, message: &str) -> Result<(), WaitError> {
        let mut lines = vec![];

        while let Some(line) = self.inner.next().await.transpose()? {
            if handle_line(line, message, &mut lines) {
                return Ok(());
            }
        }

        Err(end_of_stream(lines))
    }

    pub(crate) fn into_inner(self) -> BoxStream<'d, Result<String, std::io::Error>> {
        self.inner
    }
}

pub(crate) struct LogStream {
    inner: Box<dyn Read>,
}

impl fmt::Debug for LogStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogStream").finish()
    }
}

impl LogStream {
    pub fn new(stream: impl Read + 'static) -> Self {
        Self {
            inner: Box::new(stream),
        }
    }

    pub fn wait_for_message(self, message: &str) -> Result<(), WaitError> {
        let logs = BufReader::new(self.inner);
        let mut lines = vec![];

        for line in logs.lines() {
            if handle_line(line?, message, &mut lines) {
                return Ok(());
            }
        }

        Err(end_of_stream(lines))
    }

    pub(crate) fn into_inner(self) -> Box<dyn Read> {
        self.inner
    }
}

fn handle_line(line: String, message: &str, lines: &mut Vec<String>) -> bool {
    if line.contains(message) {
        log::info!("Found message after comparing {} lines", lines.len());

        return true;
    }

    lines.push(line);

    false
}

fn end_of_stream(lines: Vec<String>) -> WaitError {
    log::error!(
        "Failed to find message in stream after comparing {} lines.",
        lines.len()
    );

    WaitError::EndOfStream(lines)
}

/// Defines error cases when waiting for a message in a stream.
#[derive(Debug)]
pub enum WaitError {
    /// Indicates the stream ended before finding the log line you were looking for.
    /// Contains all the lines that were read for debugging purposes.
    EndOfStream(Vec<String>),
    Io(io::Error),
}

impl From<io::Error> for WaitError {
    fn from(e: io::Error) -> Self {
        WaitError::Io(e)
    }
}

pub(crate) fn get_log_dump_file_path(
    log_dump_dir: &Path,
    container_name: &str,
    stdtype: &str,
) -> PathBuf {
    // handle container names with a "/" in them, for example image names with
    // a namespace: minio/minio
    let safe_container_name = container_name.replace("/", "_");
    let log_file_name = format!("{safe_container_name}_{stdtype}.log");

    log_dump_dir.join(log_file_name)
}

pub(crate) fn get_log_dump_dir_path() -> PathBuf {
    LOGS_DUMP_DIR_PATH.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_logs_when_line_contains_message_should_find_it() {
        let log_stream = LogStream::new(
            r"
            Message one
            Message two
            Message three
        "
            .as_bytes(),
        );

        let result = log_stream.wait_for_message("Message three");

        assert!(result.is_ok())
    }
}
