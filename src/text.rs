use std::path::{Path, PathBuf};

use crate::Result;

/// A document the editor opens for read and (probably) write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The path for reading/writting into actual permanent memory.
    pub path: PathBuf,

    /// The file is read in a particular way, newlines are not included, so the
    /// file on save will have a consistent newline type, changes are made
    /// inside here.
    pub inner_lines: Vec<String>,
}

impl Document {
    /// Creates a new document with a associated path
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        /// Helper struct just to be more explicit
        struct Line {
            start: usize,
            len: usize
        }

        let bytes = std::fs::read(path.as_ref())?;
        let mut lines = Vec::new();

        // Iterate over the file and create `Line` struct to delimitate the
        // start and end of each line without including the newline symbols
        let mut start = 0;
        let mut len = 0;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\r' {
                if i < bytes.len() - 1 {
                    if bytes[i + 1] == b'\n' {
                        lines.push(Line {
                            start,
                            len
                        });

                        start = i + 2;
                        len = 0;

                        i += 2;

                        continue;
                    }
                }
            }
            
            if bytes[i] == b'\n' {
                lines.push(Line {
                    start,
                    len
                });

                start = i + 1;
                len = 0;

                i += 1;

                continue;
            }

            len += 1;
            i += 1;
        }

        // Create owned `String`s from the data to create the 
        // `self.inner_lines`
        let mut inner_lines = Vec::new();
        for line in &lines {
            inner_lines.push(
                String::from_utf8_lossy(
                    &bytes[line.start..line.start + line.len]).into_owned());
        }

        Ok(Self {
            path: path.as_ref().to_owned(),
            inner_lines,
        })
    }
}
