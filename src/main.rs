use std::io::{Write, Stdout};
use std::time::Duration;
use std::path::{Path, PathBuf};

use crossterm::{QueueableCommand, queue, execute, terminal};
use crossterm::style::{Print, PrintStyledContent, Color, Stylize, Attribute};
use crossterm::event::*;

/// Wrapper around Result
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct EditorState {
    doc_lines: usize,
    running: bool,
    rows: usize,
    columns: usize,
}

struct CursorState {
    last_column: bool,
    last_padding: usize,
    scroll_y: usize,
}

struct RenderState {
    modif_column: Option<usize>,
    modif_all: bool,
    last_cursor: Option<Cursor>
}

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
        CursorState {
            last_column,
            last_padding,
            scroll_y,
        }: &mut CursorState
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

    /// Adjust the column when a vertical movement issued to be at the 
    /// begining of the text, taking into account padding.
    fn adjust_column_start(
        &mut self,
        doc: &Document,
        CursorState {
            last_padding,
            scroll_y,
            ..
        }: &mut CursorState
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

        // Update the cursor column to the start
        self.column = curr_padding;
    }

    /// Adjust the column when a vertical movement issued to be at the 
    /// end of the line
    fn adjust_column_end(
        &mut self,
        doc: &Document,
        CursorState {
            last_column,
            last_padding,
            scroll_y
        }: &mut CursorState
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

        // We want the next left movement to go left
        *last_column = false;

        // Update the cursor column
        self.column = curr_line.len().checked_sub(1).unwrap_or(0);
    }

    /// Adjust the column when a random movement occurs, mouse for example, it
    /// ensures that the column doesn't exceeds the line width and updates some
    /// metadata needed for the next event loop iteration
    fn adjust_column_random(
        &mut self,
        doc: &Document,
        CursorState { last_column, last_padding, scroll_y }: &mut CursorState
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

    /// Move down by a single unit if possible on the file, alse update the
    /// state for the refresh
    fn move_up(
        &mut self,
        CursorState { scroll_y, .. }: &mut CursorState,
        RenderState { modif_all, last_cursor, .. }: &mut RenderState,
    ) {
        // Special case cursor at the top of the terminal, 
        // so try to scroll (if possible)
        if self.row == 0 {
            if *scroll_y > 0 {
                *modif_all = true;
                *scroll_y -= 1;
            }

        // Normal up
        } else {
            *last_cursor = Some(*self);
            self.row -= 1;
        }
    }

    /// Move up by a single unit if possible on the file, alse update the state
    /// for the refresh
    fn move_down(
        &mut self,
        EditorState { rows, doc_lines, .. }: &EditorState,
        CursorState { scroll_y, .. }: &mut CursorState,
        RenderState { modif_all, last_cursor, .. }: &mut RenderState
    ) {
        // Special case the cursor is at the bottom of the
        // terminal, scroll if possible
        if self.row == *rows {
            if *scroll_y < *doc_lines - *rows {
                *modif_all = true;
                *scroll_y += 1;
            }
        } else {
            // Special case of empty file
            if *scroll_y != 0 || *doc_lines != 0 {
                // Normal move down
                if self.row < *doc_lines - *scroll_y - 1 {
                    *last_cursor = Some(*self);
                    self.row += 1;
                }
            }
        }
    }

    /// Move the scroll up by an entire page leaving the cursor on its position
    fn page_up(
        &mut self,
        EditorState { rows, .. }: &EditorState,
        CursorState { scroll_y, .. }: &mut CursorState,
        RenderState { modif_all, last_cursor, .. }: &mut RenderState
    ) {
        // Normal page up
        if scroll_y.checked_sub(*rows).is_some() {
            *modif_all = true;
            *scroll_y -= *rows;

        // Special case you only can get to the top of 
        // the terminal
        } else {
            *last_cursor = Some(*self);            

            *scroll_y = 0;
            self.row = 0;
        }
    }

    /// Move the scroll down by an entire page leaving the cursor on its 
    /// position
    fn page_down(
        &mut self,
        EditorState { rows, doc_lines, .. }: &EditorState,
        CursorState { scroll_y, .. }: &mut CursorState,
        RenderState { modif_all, last_cursor, .. }: &mut RenderState
    ) {
        if *scroll_y + *rows <= *doc_lines {
            *modif_all = true;
            *scroll_y += *rows;
        } else {
            *last_cursor = Some(*self);
            self.row = (doc_lines % rows)
                .checked_sub(1)
                .unwrap_or(0);
        }
    }

    fn scroll_down(
        &mut self,
        EditorState { rows, doc_lines, .. }: &EditorState,
        CursorState { scroll_y, .. }: &mut CursorState,
        RenderState { modif_all, .. }: &mut RenderState
    ) {
        // Special case the cursor is at the bottom of the
        // terminal, scroll if possible
        if self.row == *rows {
            if *scroll_y < doc_lines - rows {
                *modif_all = true;
                *scroll_y += 1;
                self.row -= 1;
            }
        } else {
            // Special case of empty file
            if *scroll_y != 0 || *doc_lines != 0 {
                // Normal move down
                if self.row < doc_lines - *scroll_y - 1 {
                    *scroll_y += 1;
                    if self.row != 0 {
                        self.row -= 1;
                    }
                }

            }
        }
    }

    fn scroll_up(
        &mut self,
        EditorState { rows , .. }: &EditorState,
        CursorState { scroll_y, .. }: &mut CursorState,
        RenderState { modif_all, .. }: &mut RenderState
    ) { 
        // Special case cursor at the top of the terminal, so
        // try to scroll (if possible)
        if self.row == 0 {
            if *scroll_y > 0 {
                *modif_all = true;

                *scroll_y -= 1;
                self.row += 1;
            }

        // Normal up
        } else if *scroll_y > 0 {
            *modif_all = true;

            *scroll_y -= 1;
            if self.row != *rows {
                self.row += 1;
            }
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
    EditorState { rows, columns, .. }: &EditorState,
    CursorState { scroll_y, .. }: &CursorState,
    _: &RenderState
) -> Result<()> {
    // Hide the cursor
    execute!(stdout, crossterm::cursor::Hide)?;

    // Re-draw all the rows
    for row in 0..(*rows - 1) as u16 {
        // Clear this line
        queue!(stdout, 
            crossterm::cursor::MoveToRow(row),
            terminal::Clear(terminal::ClearType::CurrentLine))?;

        queue!(stdout,
            crossterm::cursor::MoveTo(0, row),
            Print("~ "
                .with(Color::Yellow)))?;

        // Print the document
        if let Some(doc) = document {
            if let Some(line) = &doc.inner_lines.get(row as usize + scroll_y) {
                queue!(stdout,
                    crossterm::cursor::MoveTo(0, row),
                    PrintStyledContent(format!("{:3} ", row as usize + scroll_y)
                        .with(Color::Yellow)),
                    Print(line))?;
            }
        } else {
            // Print the intro (no document opened)
            if row == *rows as u16 / 3 {
                let msg = "Pepe editor -- version 0.0.1"
                    .with(Color::Blue);
                let msg_start = *columns / 2 - msg.content().len() / 2;

                queue!(stdout,
                    crossterm::cursor::MoveTo(msg_start as u16, row),
                    PrintStyledContent(msg))?;
            }
        }
    }

    // Print the status bar
    //
    // TODO: Modifications in-place of the `status_msg` might improve perf
    let mut status_msg = String::with_capacity(*columns);
    if let Some(doc) = document {
        // Insert the path and a couple whitespaces, not sure if the conversion
        // from path -> str can really fail
        status_msg.push_str(doc.path.to_str().unwrap());

        // Create the sub-string with the cursor location + percentage of file
        // explored
        let percentage = 
            (scroll_y + cursor.row) as f32 / doc.inner_lines.len() as f32;

        // It must have a whitespace inside always, so look for it and insert
        // more whitespaces until `doc_status` can fill all the remaining 
        // status bar characters
        let doc_status = format!("{},{}{:8}%", 
                cursor.column, cursor.row, (percentage * 100.0) as u32);
        let mut ws_between = columns.checked_sub(
                                    status_msg.len())
                                        .and_then(|x| 
                                            x.checked_sub(doc_status.len()))
                                    .unwrap_or(0);
        if ws_between != 0 {
            while ws_between != 0 {
                status_msg.push(' ');
                ws_between -= 1;
            }
            status_msg.push_str(&doc_status);
        }
    } else {
        // On case no document loaded the status bar is this simple
        status_msg.push_str("[blank]");
        for _ in 0..columns - 7 {
            status_msg.push(' ');
        }
    }
    queue!(stdout,
        crossterm::cursor::MoveToNextLine(1),
        PrintStyledContent(status_msg
            .with(Color::Black)
            .on(Color::White)))?;

    // Show again the cursor
    queue!(stdout, 
        crossterm::cursor::MoveTo(cursor.column as u16 + 4, cursor.row as u16),
        crossterm::cursor::Show)?;

    // Send all the draw commands at once
    stdout.flush()?;

    Ok(())
}

// TODO: Use async like a real castellanoleonés
fn process_keypress(
    doc: &mut Option<Document>,
    cursor: &mut Cursor,
    editor_state: &mut EditorState,
    cursor_state: &mut CursorState,
    render_state: &mut RenderState,
) -> Result<()> {
    // Extract the size of the working buffer and update the editor state
    let (columns, rows) = terminal::size()?;
    let rows = (rows - 2) as usize;
    let columns = (columns - 4) as usize;
    editor_state.rows = rows;
    editor_state.columns = columns;

    if let Ok(true) = poll(Duration::from_millis(50)) {
        if let Ok(ref event) = read() {
            match event {
                Event::Key(KeyEvent {
                    code: KeyCode::Char('q'),
                    ..
                }) => editor_state.running = false,
                Event::Key(KeyEvent {
                    code: KeyCode::Up,
                    modifiers
                }) => {
                    // Page up
                    if modifiers.contains(KeyModifiers::SHIFT) {
                        cursor.page_up(
                            editor_state, cursor_state, render_state);

                    // Normal Up: go to previous line, to the same column 
                    // or closest to end of line
                    } else {
                        cursor.move_up(
                            cursor_state, 
                            render_state);
                    }

                    // Adjust the move up on the file to the proper column
                    if let Some(doc) = doc {
                        cursor.adjust_column_vertical(
                            &doc, 
                            *modifiers, 
                            cursor_state);
                    } else {
                        cursor.row = 0;
                    }
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Down,
                    modifiers
                }) => {
                    // Page down
                    if modifiers.contains(KeyModifiers::SHIFT) {
                        cursor.page_down(
                            &editor_state,
                            cursor_state,
                            render_state);

                    // Normal Down: go to next line, to the same column or
                    // closest to end of line
                    } else {
                        cursor.move_down(
                            editor_state, 
                            cursor_state, 
                            render_state);
                    }

                    // Adjust the move down on the file to the proper column
                    if let Some(doc) = doc {
                        cursor.adjust_column_vertical(
                            &doc,
                            *modifiers,
                            cursor_state);
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
                        // the document, needed to get the maximum column or 
                        // for the simple word advance
                        let curr_line = 
                            &doc.inner_lines[cursor_state.scroll_y + cursor.row];
                        let max_col = curr_line.len()
                                .checked_sub(1).unwrap_or(0);

                        // Bounds check
                        if cursor.row == 
                                editor_state.doc_lines -
                                    cursor_state.scroll_y - 1 {
                            return Ok(());
                        }

                        // This applies to all word movements, if at the end of
                        // line do a Normal down
                        if cursor.column == max_col {
                            render_state.last_cursor = Some(*cursor);

                            // Normal move down
                            cursor.move_down(
                                editor_state,
                                cursor_state, 
                                render_state);

                            // Adjust the move down on the file to the proper
                            // column
                            cursor.adjust_column_start(
                                &doc, 
                                cursor_state);
                            return Ok(());
                        }

                        render_state.last_cursor = Some(*cursor);
                        // Simple word movement (until next whitespace)
                        if modifiers.contains(KeyModifiers::CONTROL) {
                            match curr_line.as_bytes()[cursor.column] {
                                b' ' => {
                                    let mut new_col = cursor.column;
                                    while new_col <= max_col && 
                                        curr_line
                                            .as_bytes()[new_col] 
                                            == b' ' {
                                                new_col += 1;
                                    }

                                    cursor.column = 
                                        usize::min(max_col, new_col);
                                },
                                _ => {
                                    let mut new_col = cursor.column;
                                    while new_col < max_col &&
                                          curr_line
                                              .as_bytes()[new_col]
                                                    != b' ' {
                                        new_col += 1;
                                    }
                                    while new_col < max_col &&
                                          curr_line
                                              .as_bytes()[new_col]
                                                    == b' ' {
                                        new_col += 1;
                                    }

                                    cursor.column = 
                                        usize::min(max_col, new_col);
                                }
                            }

                        // Normal cursor movement 
                        } else {
                            cursor.column = 
                                usize::min(max_col, cursor.column + 1);
                        }

                        // Needed to handle the case last movement was at end
                        // of line and you go up/down and need to still be at
                        // the end of line
                        if cursor.column == max_col {
                            cursor_state.last_column = true;
                        }
                    }
                } 
                Event::Key(KeyEvent {
                    code: KeyCode::Left,
                    modifiers
                }) => {
                    // Every movement to the left means no more end of line
                    cursor_state.last_column = false;

                    if let Some(doc) = doc {
                        // This applies to all word movements, if at the end of
                        // line just try going to the next
                        if cursor.column == 0 {
                            // Normal move up
                            cursor.move_up(
                                cursor_state, 
                                render_state);

                            // Adjust the move down on the file to the proper
                            // column
                            if cursor.row != 0 {
                                cursor.adjust_column_end(
                                    &doc,
                                    cursor_state);
                            }

                            return Ok(());
                        }

                        // Get a reference to the line the cursor is at on the
                        // document
                        let curr_line = 
                            &doc.inner_lines[cursor_state.scroll_y + cursor.row];

                        // Bounds check
                        if cursor.row 
                                == editor_state.doc_lines -
                                        cursor_state.scroll_y - 1 {
                            return Ok(());
                        }

                        // Simple word movement (until next whitespace)
                        if modifiers.contains(KeyModifiers::CONTROL) {
                            match curr_line.as_bytes()[cursor.column] {
                                b' ' => {
                                    let mut new_col = cursor.column;
                                    while new_col != 0 && 
                                          curr_line
                                              .as_bytes()[new_col] 
                                                    == b' ' {
                                        new_col -= 1;
                                    }

                                    cursor.column = new_col;
                                },
                                _ => {
                                    let mut new_col = cursor.column;
                                    while new_col != 0 &&
                                          curr_line
                                              .as_bytes()[new_col]
                                                    != b' ' {
                                        new_col -= 1;
                                    }
                                    while new_col != 0 && 
                                          curr_line
                                              .as_bytes()[new_col] 
                                                    == b' ' {
                                        new_col -= 1;
                                    }

                                    cursor.column = new_col;
                                }
                            }

                        // Normal cursor movement
                        } else {
                            cursor.column = usize::max(0, cursor.column
                                .checked_sub(1).unwrap_or(0));
                        }
                    }
                }

                // Handle scroll up/down
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollUp,
                    modifiers,
                    ..
                }) => {
                    // Page up
                    if modifiers.contains(KeyModifiers::SHIFT) {
                        cursor.page_up(
                            editor_state, 
                            cursor_state, 
                            render_state);

                    // Scroll Up
                    } else {
                        cursor.scroll_up(
                            editor_state,
                            cursor_state,
                            render_state);
                    }

                    // Adjust the move up on the file to the proper column
                    if let Some(doc) = doc {
                        cursor.adjust_column_vertical(
                            &doc, 
                            *modifiers,
                            cursor_state);
                    } else {
                        cursor.row = 0;
                    }
                }
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollDown,
                    modifiers,
                    ..
                }) => {
                    // Page down
                    if modifiers.contains(KeyModifiers::SHIFT) {
                        cursor.page_down(
                            editor_state,
                            cursor_state,
                            render_state);

                    // Scroll Down
                    } else {
                        cursor.scroll_down(
                            editor_state,
                            cursor_state,
                            render_state);
                    }

                    // Adjust the move down on the file to the proper 
                    // column
                    if let Some(doc) = doc {
                        cursor.adjust_column_vertical(
                            &doc, 
                            *modifiers, 
                            cursor_state);
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
                    render_state.last_cursor = Some(*cursor);

                    // Translate the terminal coords to buffer coords
                    let row = usize::min(*row as usize, 
                        (editor_state.doc_lines % editor_state.rows)
                        .checked_sub(1).unwrap_or(0));
                    let column = usize::min(
                                    column.checked_sub(4).unwrap_or(0) as usize,
                                    editor_state.columns);

                    render_state.last_cursor = Some(*cursor);

                    cursor.row = row;
                    cursor.column = column;

                    if let Some(doc) = doc {
                        cursor.adjust_column_random(
                            &doc, 
                            cursor_state);
                    } else {
                        cursor.row = 0;
                        cursor.column = 0;
                    }
                }
                _ => {}
            }
        }
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
    let doc_lines;
    let mut curr_doc = if let Some(path) = path {
        let doc = Document::new(path)?;
        doc_lines = doc.inner_lines.len();
        Some(doc)
    } else {
        doc_lines = 0;
        None
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
        terminal::EnableLineWrap,
        crossterm::cursor::DisableBlinking,
        EnableMouseCapture)?;

    // Initial cursor position
    let mut cursor = Cursor {
        column: 0,
        row: 0
    };

    // Editor state
    let (columns, rows) = terminal::size()?;
    let rows = (rows - 2) as usize;
    let columns = (columns - 4) as usize;
    let mut editor_state = EditorState {
        running: true,
        rows,
        columns,
        doc_lines
    };

    // Cursor state needed to calculate movement
    let mut cursor_state = CursorState {
        scroll_y: 0,
        last_column: false,
        last_padding: 0
    };

    // Render state to update the screen efficiently
    let mut render_state = RenderState {
        modif_column: None,
        modif_all: false,
        last_cursor: None
    };

    loop {
        // Repaint on the screen what needs to be repainted
        refresh_screen(
            &mut stdout,
            &curr_doc,
            &cursor,
            &editor_state,
            &cursor_state,
            &mut render_state)?;

        render_state.last_cursor = None;

        // Check if the editor should keep running, if it should close it will
        // clear all it drawed
        if !editor_state.running {
            break;
        }

        // Process events
        process_keypress(
            &mut curr_doc,
            &mut cursor,
            &mut editor_state,
            &mut cursor_state,
            &mut render_state)?;

        render_state.modif_column = None;
        render_state.modif_all = false;
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
