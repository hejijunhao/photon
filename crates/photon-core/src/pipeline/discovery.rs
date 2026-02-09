//! File discovery for finding images in directories.

use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::config::ProcessingConfig;

/// Discovers image files in directories.
pub struct FileDiscovery {
    config: ProcessingConfig,
}

/// Information about a discovered file.
#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    /// Full path to the file
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
}

impl FileDiscovery {
    /// Create a new file discovery instance.
    pub fn new(config: ProcessingConfig) -> Self {
        Self { config }
    }

    /// Discover all supported image files at a path.
    ///
    /// If path is a file, returns it if supported.
    /// If path is a directory, recursively finds all supported files.
    pub fn discover(&self, path: &Path) -> Vec<DiscoveredFile> {
        if path.is_file() {
            if self.is_supported(path) {
                if let Ok(meta) = std::fs::metadata(path) {
                    return vec![DiscoveredFile {
                        path: path.to_path_buf(),
                        size: meta.len(),
                    }];
                }
            }
            return vec![];
        }

        let mut files = Vec::new();

        for entry in WalkDir::new(path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let entry_path = entry.path();
            if entry_path.is_file() && self.is_supported(entry_path) {
                if let Ok(meta) = entry.metadata() {
                    files.push(DiscoveredFile {
                        path: entry_path.to_path_buf(),
                        size: meta.len(),
                    });
                }
            }
        }

        // Sort by path for deterministic ordering
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files
    }

    /// Check if a file has a supported extension.
    fn is_supported(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                let ext_lower = ext.to_lowercase();
                self.config
                    .supported_formats
                    .iter()
                    .any(|fmt| fmt.to_lowercase() == ext_lower)
            })
            .unwrap_or(false)
    }

    /// Get total size of all discovered files.
    pub fn total_size(files: &[DiscoveredFile]) -> u64 {
        files.iter().map(|f| f.size).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported() {
        let config = ProcessingConfig::default();
        let discovery = FileDiscovery::new(config);

        assert!(discovery.is_supported(Path::new("test.jpg")));
        assert!(discovery.is_supported(Path::new("test.JPG")));
        assert!(discovery.is_supported(Path::new("test.jpeg")));
        assert!(discovery.is_supported(Path::new("test.png")));
        assert!(discovery.is_supported(Path::new("test.webp")));
        assert!(!discovery.is_supported(Path::new("test.txt")));
        assert!(!discovery.is_supported(Path::new("test.pdf")));
    }

    #[test]
    fn test_total_size() {
        let files = vec![
            DiscoveredFile {
                path: PathBuf::from("a.jpg"),
                size: 100,
            },
            DiscoveredFile {
                path: PathBuf::from("b.jpg"),
                size: 200,
            },
        ];

        assert_eq!(FileDiscovery::total_size(&files), 300);
    }
}
