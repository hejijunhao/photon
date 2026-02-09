//! Output formatting for JSON and JSONL output.
//!
//! Provides a flexible writer that can output single items or batches
//! in either JSON or JSON Lines format.

use serde::Serialize;
use std::io::{self, Write};

/// Output format options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Single JSON object or array
    Json,
    /// One JSON object per line (newline-delimited JSON)
    JsonLines,
}

impl OutputFormat {
    /// Parse format from string (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "json" => Some(Self::Json),
            "jsonl" | "jsonlines" | "ndjson" => Some(Self::JsonLines),
            _ => None,
        }
    }
}

/// A writer that serializes items to JSON or JSONL format.
pub struct OutputWriter<W: Write> {
    writer: W,
    format: OutputFormat,
    pretty: bool,
    items_written: usize,
}

impl<W: Write> OutputWriter<W> {
    /// Create a new output writer.
    ///
    /// # Arguments
    ///
    /// * `writer` - The underlying writer (file, stdout, etc.)
    /// * `format` - Output format (JSON or JSONL)
    /// * `pretty` - Whether to pretty-print JSON (only affects JSON format)
    pub fn new(writer: W, format: OutputFormat, pretty: bool) -> Self {
        Self {
            writer,
            format,
            pretty,
            items_written: 0,
        }
    }

    /// Write a single item.
    ///
    /// For JSON format, writes a single object.
    /// For JSONL format, writes one object per line.
    pub fn write<T: Serialize>(&mut self, item: &T) -> io::Result<()> {
        match self.format {
            OutputFormat::Json => {
                if self.pretty {
                    serde_json::to_writer_pretty(&mut self.writer, item)
                        .map_err(io::Error::other)?;
                } else {
                    serde_json::to_writer(&mut self.writer, item).map_err(io::Error::other)?;
                }
                writeln!(self.writer)?;
            }
            OutputFormat::JsonLines => {
                // JSONL is never pretty-printed (one object per line)
                serde_json::to_writer(&mut self.writer, item).map_err(io::Error::other)?;
                writeln!(self.writer)?;
            }
        }
        self.items_written += 1;
        Ok(())
    }

    /// Write multiple items.
    ///
    /// For JSON format, writes as a JSON array.
    /// For JSONL format, writes one object per line.
    pub fn write_all<T: Serialize>(&mut self, items: &[T]) -> io::Result<()> {
        match self.format {
            OutputFormat::Json => {
                if self.pretty {
                    serde_json::to_writer_pretty(&mut self.writer, items)
                        .map_err(io::Error::other)?;
                } else {
                    serde_json::to_writer(&mut self.writer, items).map_err(io::Error::other)?;
                }
                writeln!(self.writer)?;
                self.items_written += items.len();
            }
            OutputFormat::JsonLines => {
                for item in items {
                    self.write(item)?;
                }
            }
        }
        Ok(())
    }

    /// Get the number of items written.
    pub fn items_written(&self) -> usize {
        self.items_written
    }

    /// Flush the underlying writer.
    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    /// Consume the writer and return the underlying writer.
    pub fn into_inner(self) -> W {
        self.writer
    }
}

/// Convenience function to serialize an item to a JSON string.
pub fn to_json<T: Serialize>(item: &T, pretty: bool) -> Result<String, serde_json::Error> {
    if pretty {
        serde_json::to_string_pretty(item)
    } else {
        serde_json::to_string(item)
    }
}

/// Convenience function to serialize items to JSONL format.
pub fn to_jsonl<T: Serialize>(items: &[T]) -> Result<String, serde_json::Error> {
    let mut output = String::new();
    for item in items {
        output.push_str(&serde_json::to_string(item)?);
        output.push('\n');
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct TestItem {
        name: String,
        value: i32,
    }

    #[test]
    fn test_write_json() {
        let mut buffer = Vec::new();
        let mut writer = OutputWriter::new(&mut buffer, OutputFormat::Json, false);

        let item = TestItem {
            name: "test".to_string(),
            value: 42,
        };
        writer.write(&item).unwrap();

        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("\"name\":\"test\""));
        assert!(output.contains("\"value\":42"));
    }

    #[test]
    fn test_write_jsonl() {
        let mut buffer = Vec::new();
        let mut writer = OutputWriter::new(&mut buffer, OutputFormat::JsonLines, false);

        let items = vec![
            TestItem {
                name: "a".to_string(),
                value: 1,
            },
            TestItem {
                name: "b".to_string(),
                value: 2,
            },
        ];

        for item in &items {
            writer.write(item).unwrap();
        }

        let output = String::from_utf8(buffer).unwrap();
        let lines: Vec<&str> = output.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_write_all_json_array() {
        let mut buffer = Vec::new();
        let mut writer = OutputWriter::new(&mut buffer, OutputFormat::Json, false);

        let items = vec![
            TestItem {
                name: "a".to_string(),
                value: 1,
            },
            TestItem {
                name: "b".to_string(),
                value: 2,
            },
        ];

        writer.write_all(&items).unwrap();

        let output = String::from_utf8(buffer).unwrap();
        assert!(output.starts_with('['));
        assert!(output.trim().ends_with(']'));
    }

    #[test]
    fn test_format_parse() {
        assert_eq!(OutputFormat::parse("json"), Some(OutputFormat::Json));
        assert_eq!(OutputFormat::parse("jsonl"), Some(OutputFormat::JsonLines));
        assert_eq!(OutputFormat::parse("JSONL"), Some(OutputFormat::JsonLines));
        assert_eq!(OutputFormat::parse("invalid"), None);
    }
}
