use std::io::Stdout;
use std::time::Duration;
use std::path::{Path, PathBuf};

use crossterm::{QueueableCommand, execute, terminal};
use crossterm::style::{Print, PrintStyledContent, Color, Stylize, Attribute};
use crossterm::event::*;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Represents the position of a cursor on a file or the terminal screen
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Position {
    column: usize,
    row: usize
}

/// A document the editor opens for read and (probably) write.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Document {
    /// The path for reading/writting into actual permanent memory.
    path: PathBuf,

    /// The file is read in a particular way, newlines are not included, so the
    /// file on save will have a consistent newline type, changes are made
    /// inside here.
    inner_lines: Vec<String>,
}

impl Document {
    /// Creates a new document with a associated path
    fn new(path: impl AsRef<Path>) -> Result<Self> {
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
                    if bytes[i] == b'\n' {
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

/// Update (if needed) the elements that need to be updated on the screen
fn refresh_screen(
    stdout: &mut Stdout,
    document: &Option<Document>,
    cursor: &Position,
    scroll_y: usize,
) -> Result<()> {
    // Hide the cursor
    execute!(stdout, crossterm::cursor::Hide)?;

    // Re-draw all the rows
    let (columns, rows) = terminal::size()?;
    for row in 0..rows {
        // Clear this line
        execute!(stdout, 
            crossterm::cursor::MoveTo(0, row),
            terminal::Clear(terminal::ClearType::CurrentLine))?;

        execute!(stdout,
            crossterm::cursor::MoveTo(0, row),
            Print("~ "
                .with(Color::Yellow)))?;

        // Print the document
        if let Some(doc) = document {
            if let Some(line) = &doc.inner_lines.get(row as usize + scroll_y) {
                execute!(stdout,
                    crossterm::cursor::MoveTo(0, row),
                    PrintStyledContent(format!("{:3} ", row as usize + scroll_y)
                        .with(Color::Yellow)),
                    Print(line));
            }
        } else {
            // Print the intro (no document opened)
            if row == rows / 3 {
                let msg = "Hecto editor -- version 0.0.1"
                    .with(Color::Blue);
                assert!(columns / 2 >= msg.content().len() as u16 / 2);
                let msg_start = columns / 2 - msg.content().len() as u16 / 2;

                execute!(stdout,
                    crossterm::cursor::MoveTo(msg_start, row),
                    PrintStyledContent(msg))?;
            }
        }
    }

    // Show again the cursor
    execute!(stdout, 
        crossterm::cursor::MoveTo(cursor.column as u16, cursor.row as u16),
        crossterm::cursor::Show)?;
    
    Ok(())
}

// TODO: Use async like a real castellanoleonés
fn process_keypress(
    running: &mut bool, 
    cursor: &mut Position,
    scroll_y: &mut usize,
    doc_lines: usize,
) -> Result<()> {
    match poll(Duration::from_millis(50)) {
        Ok(true) => {
            if let Ok(ref event) = read() {
                match event {
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('q'),
                        ..
                    }) => *running = false,
                    Event::Key(KeyEvent {
                        code: KeyCode::Up,
                        ..
                    }) => {
                        if cursor.row == 0 {
                            if *scroll_y > 0 {
                                *scroll_y -= 1;
                            }
                        } else {
                            cursor.row -= 1;
                        }
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Down,
                        ..
                    }) => {
                        let (columns, rows) = terminal::size()?;
                        if cursor.row == rows as usize {
                            if *scroll_y < doc_lines - rows as usize {
                                *scroll_y += 1;
                            }
                        } else {
                            if cursor.row < doc_lines - *scroll_y - 1 {
                                cursor.row += 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
        },
        _ => {}
    }

    Ok(())
}

fn main() -> Result<()> {
    // Extract the path of the file to edit and open it as a `Document`
    let path = std::env::args()
        // Extract just the fist argument
        .nth(1)
        // Convert it to a path
        .map(|s| PathBuf::from(s));
    let mut curr_doc = if let Some(path) = path {
        Some(Document::new(path)?)
    } else {
        None
    };

    // Initial position of the cursor
    let mut cursor = Position {
        column: 0,
        row: 0
    };

    // Put the terminal in raw mode, which means that:
    //  - The stdin doesn't go the stdout directly, its buffered.
    //  - Any special characters `Ctrl + ...` has no special behaviours.
    //  - New lines have no effect on the stdout, so their type must be 
    //    explicit by sending a command to newline.
    terminal::enable_raw_mode()?;

    // Enable mouse support
    let mut stdout = std::io::stdout();
    execute!(stdout, EnableMouseCapture)?;

    let mut doc_lines = match curr_doc {
        Some(ref doc) => doc.inner_lines.len(),
        None => 0,
    };
    let mut scroll_y = 0;
    let mut running = true;
    loop {
        // Repaint on the screen what needs to be repainted
        refresh_screen(&mut stdout, &curr_doc, &cursor, scroll_y)?;

        // Check if the editor should keep running, if it should close it will
        // clear all it drawed
        if !running {
            execute!(stdout, terminal::Clear(terminal::ClearType::Purge))?;
            break;
        }

        // Process events
        process_keypress(&mut running, &mut cursor, &mut scroll_y, doc_lines)?;
    }

    // Back to normal terminal after closing
    terminal::disable_raw_mode()?;

    Ok(())
}
