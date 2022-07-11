use std::io::Stdout;
use std::time::Duration;
use std::path::{Path, PathBuf};

use crossterm::{QueueableCommand, execute, terminal};
use crossterm::style::{Print, PrintStyledContent, Color, Stylize, Attribute};
use crossterm::event::*;

/// Wrapper around Result
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Represents the cursor on the terminal screen
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cursor {
    column: usize,
    row: usize
}

impl Cursor {
    /// Adjust the column when a vertical movement issued to the proper column.
    ///
    /// Cases:
    ///     - When the last line column was the first/last one, this will 
    ///       adjust the column of the cursor to the first/last line of the 
    ///       current line
    ///     - Take into account also the last padding, so if the whitespaces
    ///       on the left incrase the column is incrased by that incrase
    ///       ammount
    fn adjust_column_vertical(
        &mut self,
        doc: &Document,
        modifiers: KeyModifiers,
        scroll_y: &mut usize,
        last_column: &mut bool,
        last_padding: &mut usize
    ) {
        // Get a reference to the line the cursor is at on the document
        let curr_line = 
            &doc.inner_lines[*scroll_y + self.row];

        // Update the padding
        let mut curr_padding = 0;
        while curr_padding < curr_line.len() 
                && curr_line
                    .as_bytes()[curr_padding]
                    .is_ascii_whitespace() {
            curr_padding += 1;
        }
        let padding = curr_padding as i32 - *last_padding as i32;

        // If control pressed, just go forward without padding calculations
        let new_column;
        if !modifiers.contains(KeyModifiers::CONTROL) {
            new_column = (self.column as i32 + padding)
                .try_into().unwrap_or(0);
        } else {
            new_column = self.column;
        }

        *last_padding = curr_padding;

        // Update the cursor position knowning that, also handling the case 
        // that the last movement was on last line, so this will also be on the 
        // last line
        let max_col = curr_line.len().checked_sub(1).unwrap_or(0);
        if *last_column {
            self.column = max_col;
        } else {
            self.column = usize::min(max_col, new_column);
        }
    }

    /// Adjust the column when a random movement occurs, mouse for example, it
    /// ensures that the column doesn't exceeds the line width and updates some
    /// metadata needed for the next event loop iteration
    fn adjust_column_random(
        &mut self,
        doc: &Document,
        scroll_y: &usize,
        last_column: &mut bool,
        last_padding: &mut usize
    ) {
        // Get a reference to the line the cursor is at on the document
        let curr_line = 
            &doc.inner_lines[*scroll_y + self.row];

        // Update the padding
        let mut curr_padding = 0;
        while curr_padding < curr_line.len() 
                && curr_line
                    .as_bytes()[curr_padding]
                    .is_ascii_whitespace() {
            curr_padding += 1;
        }
        *last_padding = curr_padding;

        // Update the `last_column` and the column if exceeds the line width
        let max_col = curr_line.len().checked_sub(1).unwrap_or(0);
        if max_col <= self.column {
            *last_column = true;
            self.column = max_col;
        }
    }
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

/// Update (if needed) the elements that need to be updated on the screen
fn refresh_screen(
    stdout: &mut Stdout,
    document: &Option<Document>,
    cursor: &Cursor,
    scroll_y: usize,
) -> Result<()> {
    // Hide the cursor
    execute!(stdout, crossterm::cursor::Hide)?;

    // Re-draw all the rows
    let (columns, rows) = terminal::size()?;
    for row in 0..rows - 1 {
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
                    Print(line))?;
            }
        } else {
            // Print the intro (no document opened)
            if row == rows / 3 {
                let msg = "Pepe editor -- version 0.0.1"
                    .with(Color::Blue);
                assert!(columns / 2 >= msg.content().len() as u16 / 2);
                let msg_start = columns / 2 - msg.content().len() as u16 / 2;

                execute!(stdout,
                    crossterm::cursor::MoveTo(msg_start, row),
                    PrintStyledContent(msg))?;
            }
        }
    }

    // Print the status bar
    //
    // TODO: Modifications in-place of the `status_msgp` might improve perf
    let mut status_msg = String::with_capacity(columns as usize);
    if let Some(doc) = document {
        // Insert the path and a couple whitespaces, not sure if the conversion
        // from path -> str can really fail
        status_msg.push_str(doc.path.to_str().unwrap());
        for _ in 0..4*columns/6 {
            status_msg.push(' ');
        }

        // Create the sub-string with the cursor location + percentage of file
        // explored
        let percentage = 
            (scroll_y + cursor.row) as f32 / doc.inner_lines.len() as f32;

        // It must have a whitespace inside always, so look for it and insert
        // more whitespaces until `doc_status` can fill all the remaining 
        // status bar characters
        let mut doc_status = format!("{},{}{:4}%", 
                cursor.column, cursor.row, (percentage * 100.0) as u32);
        let mut i = 0;
        while !doc_status.as_bytes()[i].is_ascii_whitespace() {
            i+= 1;
        }
        while status_msg.len() + doc_status.len() < columns as usize {
            doc_status.insert(i, ' ');
        }
        status_msg.push_str(&doc_status);
    } else {
        // On case no document loaded the status bar is this simple
        status_msg.push_str("[blank]");
        for _ in 0..columns - 7 {
            status_msg.push(' ');
        }
    }
    execute!(stdout,
        crossterm::cursor::MoveToNextLine(1),
        PrintStyledContent(status_msg
            .with(Color::Black)
            .on(Color::White)))?;

    // Show again the cursor
    execute!(stdout, 
        crossterm::cursor::MoveTo(cursor.column as u16 + 4, cursor.row as u16),
        crossterm::cursor::Show)?;

    Ok(())
}

// TODO: Use async like a real castellanoleonés
fn process_keypress(
    running: &mut bool, 
    cursor: &mut Cursor,
    last_column: &mut bool,
    last_padding: &mut usize,
    scroll_y: &mut usize,
    doc: &mut Option<Document>,
) -> Result<()> {
    let mut doc_lines = match doc {
        Some(ref doc) => doc.inner_lines.len(),
        None => 0,
    };

    // Extract the size of the working buffer
    let (columns, rows) = terminal::size()?;
    let rows = (rows - 2) as usize;
    let columns = (columns - 4) as usize;

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
                        modifiers
                    }) => {
                        // Shift + Up: Go up by an entire page
                        if modifiers.contains(KeyModifiers::SHIFT) {
                            // Normal page up
                            if scroll_y.checked_sub(rows).is_some() {
                                *scroll_y -= rows;
                            // Special case you only can get to the top of 
                            // the terminal
                            } else {
                                *scroll_y = 0;
                                cursor.row = 0;
                            }
                        // Normal Up: go to previous line, to the same column 
                        // or closest to end of line
                        } else {
                            // Special case cursor at the top of the terminal, 
                            // so try to scroll (if possible)
                            if cursor.row == 0 {
                                if *scroll_y > 0 {
                                    *scroll_y -= 1;
                                }
                            // Normal up
                            } else {
                                cursor.row -= 1;
                            }
                        }

                        // Adjust the move up on the file to the proper column
                        if let Some(doc) = doc {
                            cursor.adjust_column_vertical(
                                &doc,
                                *modifiers,
                                scroll_y,
                                last_column,
                                last_padding);
                        } else {
                            cursor.row = 0;
                        }
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Down,
                        modifiers
                    }) => {
                        // Shift + Down: Go down by an entire page
                        if modifiers.contains(KeyModifiers::SHIFT) {
                            if *scroll_y + rows <= doc_lines {
                                *scroll_y += rows;
                            } else {
                                cursor.row = (doc_lines % rows)
                                                .checked_sub(1)
                                                .unwrap_or(0);
                            }
                        // Normal Down: go to next line, to the same column or
                        // closest to end of line
                        } else {
                            // Special case the cursor is at the bottom of the
                            // terminal, scroll if possible
                            if cursor.row == rows {
                                if *scroll_y < doc_lines - rows {
                                    *scroll_y += 1;
                                }
                            } else {
                                // Special case of empty file
                                if *scroll_y != 0 || doc_lines != 0 {
                                    // Normal move down
                                    if cursor.row < doc_lines - *scroll_y - 1 {
                                        cursor.row += 1;
                                    }
                                }
                            }
                        }

                        // Adjust the move down on the file to the proper column
                        if let Some(doc) = doc {
                            cursor.adjust_column_vertical(
                                &doc,
                                *modifiers,
                                scroll_y,
                                last_column,
                                last_padding);
                        } else {
                            cursor.row = 0;
                        }
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Right,
                        modifiers
                    }) => {
                        if let Some(doc) = doc {
                            // Get a reference to the line the cursor is at on
                            // the document
                            let curr_line = 
                                &doc.inner_lines[*scroll_y + cursor.row];

                            // Update the cursor position knowning that
                            let max_col = curr_line.len()
                                                .checked_sub(1).unwrap_or(0);
                            cursor.column = 
                                usize::min(max_col, cursor.column + 1);

                            // Needed to handle the case last movement was at
                            // end of line and you go up/down and need to still
                            // be at the end of line
                            if cursor.column == max_col {
                                *last_column = true;
                            }
                        }
                    } 
                    Event::Key(KeyEvent {
                        code: KeyCode::Left,
                        modifiers
                    }) => {
                        // Every movement to the left means no more end
                        // of line
                        *last_column = false;

                        if let Some(doc) = doc {
                            // Get a reference to the line the cursor is at on
                            // the document
                            let curr_line = 
                                &doc.inner_lines[*scroll_y + cursor.row];

                            // Update the cursor position knowning that
                            cursor.column = 
                                usize::max(0, cursor.column
                                                .checked_sub(1).unwrap_or(0));
                        }
                    }

                    // Handle scroll up/down
                    Event::Mouse(MouseEvent {
                        kind: MouseEventKind::ScrollUp,
                        modifiers,
                        ..
                    }) => {
                        // Shift + Up: Go up by an entire page
                        if modifiers.contains(KeyModifiers::SHIFT) {
                            // Normal page up
                            if scroll_y.checked_sub(rows).is_some() {
                                *scroll_y -= rows;
                            // Special case you only can get to the top of 
                            // the terminal
                            } else {
                                *scroll_y = 0;
                                cursor.row = 0;
                            }
                        // Normal Up: go to previous line, to the same column 
                        // or closest to end of line
                        } else {
                            // Special case cursor at the top of the terminal, 
                            // so try to scroll (if possible)
                            if cursor.row == 0 {
                                if *scroll_y > 0 {
                                    *scroll_y -= 1;
                                    cursor.row += 1;
                                }
                            // Normal up
                            } else if *scroll_y > 0 {
                               *scroll_y -= 1;
                               if cursor.row != rows {
                                   cursor.row += 1;
                               }
                            }
                        }

                        // Adjust the move up on the file to the proper column
                        if let Some(doc) = doc {
                            cursor.adjust_column_vertical(
                                &doc,
                                *modifiers,
                                scroll_y,
                                last_column,
                                last_padding);
                        } else {
                            cursor.row = 0;
                        }
                    }
                    Event::Mouse(MouseEvent {
                        kind: MouseEventKind::ScrollDown,
                        modifiers,
                        ..
                    }) => {
                        // Shift + Down: Go down by an entire page
                        if modifiers.contains(KeyModifiers::SHIFT) {
                            if *scroll_y + rows <= doc_lines {
                                *scroll_y += rows;
                            } else {
                                cursor.row = (doc_lines % rows)
                                                .checked_sub(1)
                                                .unwrap_or(0);
                            }
                        // Normal Down: incrase the `scroll_y` and maintains
                        // the cursor on the same position relative to the 
                        // document
                        } else {
                            // Special case the cursor is at the bottom of the
                            // terminal, scroll if possible
                            if cursor.row == rows {
                                if *scroll_y < doc_lines - rows {
                                    *scroll_y += 1;
                                    cursor.row -= 1;
                                }
                            } else {
                                // Special case of empty file
                                if *scroll_y != 0 || doc_lines != 0 {
                                    // Normal move down
                                    if cursor.row < doc_lines - *scroll_y - 1 {
                                        *scroll_y += 1;
                                        if cursor.row != 0 {
                                            cursor.row -= 1;
                                        }
                                    }

                                }
                            }
                        }

                        // Adjust the move down on the file to the proper 
                        // column
                        if let Some(doc) = doc {
                            cursor.adjust_column_vertical(
                                &doc,
                                *modifiers,
                                scroll_y,
                                last_column,
                                last_padding);
                        } else {
                            cursor.row = 0;
                        }

                    }
                    Event::Mouse(MouseEvent {
                        kind: MouseEventKind::Up(_),
                        row,
                        column,
                        ..
                    }) => {
                        // Translate the terminal coords to buffer coords
                        let row = usize::min(*row as usize, 
                                             (doc_lines % rows)
                                                .checked_sub(1).unwrap_or(0));
                        let column = usize::min(*column as usize - 4, columns);

                        cursor.row = row;
                        cursor.column = column;

                        if let Some(doc) = doc {
                            cursor.adjust_column_random(
                                &doc,
                                scroll_y,
                                last_column,
                                last_padding);
                        } else {
                            cursor.row = 0;
                            cursor.column = 0;
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
    let mut cursor = Cursor {
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
    execute!(stdout, 
        terminal::EnterAlternateScreen,
        EnableMouseCapture)?;


    let mut last_column = false;
    let mut last_padding = 0;
    let mut scroll_y = 0;
    let mut running = true;
    loop {
        // Repaint on the screen what needs to be repainted
        refresh_screen(&mut stdout, &curr_doc, &cursor, scroll_y)?;

        // Check if the editor should keep running, if it should close it will
        // clear all it drawed
        if !running {
            break;
        }

        // Process events
        process_keypress(
            &mut running, 
            &mut cursor,
            &mut last_column,
            &mut last_padding,
            &mut scroll_y,
            &mut curr_doc)?;
    }

    // Disable mouse support and because we entered an alternative screen, when
    // we leave we resume all the output that was before the editor execution
    execute!(stdout,
        terminal::LeaveAlternateScreen,
        DisableMouseCapture)?;

    // Back to normal terminal after closing
    terminal::disable_raw_mode()?;

    Ok(())
}
