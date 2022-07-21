//! Handle all the input and the reaction of the cursor/sroll to it

use std::time::Duration;

use crossterm::terminal;
use crossterm::event::*;

use crate::{EditorState, Result};
use crate::text::Document;
use crate::render::RenderState;

#[repr(u32)]
enum BeepType {
    Simple = 0xFFFFFFFF,
    Asterisk = 0x00000010,
}

#[link(name="User32")]
extern "C" {
    fn MessageBeep(uType: u32) -> i32;
}

macro_rules! beep {
    (0) => {
        unsafe {
            MessageBeep(BeepType::Simple as u32);
        }
    };
    (1) => {
        unsafe {
            MessageBeep(BeepType::Asterisk as u32);
        }
    };
}

/// The state of the cursor, needed to handle the movements properly
pub struct CursorState {
    /// Not only if the cursor is at the last column, but if it should behave
    /// like it, changing how the up/down movemnts work
    pub last_column: bool,

    /// Used to calculate the column, is the last space padding
    pub last_padding: usize,

    /// The scrolling on the terminal
    pub scroll_y: usize,
}

/// Represents the cursor on the terminal screen
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub column: usize,
    pub row: usize
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
    pub fn adjust_column_vertical(
        &mut self,
        doc: &Document,
        modifiers: KeyModifiers,
        CursorState { last_column, last_padding, scroll_y }: &mut CursorState
    ) {
        // Get a reference to the line the cursor is at on the document
        assert!(*scroll_y < 134);
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
    pub fn adjust_column_start(
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
    pub fn adjust_column_end(
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

        // We want the next left movement to go left
        *last_column = false;

        // Update the cursor column
        self.column = curr_line.len().checked_sub(1).unwrap_or(0);
    }

    /// Adjust the column when a random movement occurs, mouse for example, it
    /// ensures that the column doesn't exceeds the line width and updates some
    /// metadata needed for the next event loop iteration
    pub fn adjust_column_random(
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
    pub fn move_up(
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
    pub fn move_down(
        &mut self,
        EditorState { rows, doc_lines, .. }: &EditorState,
        CursorState { scroll_y, .. }: &mut CursorState,
        RenderState { modif_all, last_cursor, .. }: &mut RenderState
    ) {
        // Special case the cursor is at the bottom of the
        // terminal, scroll if possible
        if self.row == *rows - 1 {
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
    pub fn page_up(
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
    pub fn page_down(
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

    pub fn scroll_down(
        &mut self,
        EditorState { rows, doc_lines, .. }: &EditorState,
        CursorState { scroll_y, .. }: &mut CursorState,
        RenderState { modif_all, .. }: &mut RenderState
    ) {
        // Special case of empty file
        if *scroll_y != 0 || *doc_lines != 0 {
            // Normal scroll down
            if self.row < doc_lines - *scroll_y - 1 {
                *modif_all = true;

                *scroll_y += 1;
                if self.row != 0 {
                    self.row -= 1;
                }
            }
        }
    }

    pub fn scroll_up(
        &mut self,
        EditorState { rows , .. }: &EditorState,
        CursorState { scroll_y, .. }: &mut CursorState,
        RenderState { modif_all, .. }: &mut RenderState
    ) { 
        // Special case cursor at the top of the terminal, so
        // try to scroll (if possible)
        if self.row == 0 {
            beep!(1);
            if *scroll_y > 0 {
                *modif_all = true;

                *scroll_y -= 1;
                self.row += 1;
            }

        // Normal up
        } else if *scroll_y >= 0 {
            *modif_all = true;
            

            *scroll_y -= 1;
            if self.row != *rows - 1 {
                self.row += 1;
            }  
        } else {
            /*
            beep!(0);
            dbg!(scroll_y);
            panic!();
            */
        }
    }
}

// TODO: Use async like a real castellanoleonés
pub fn process_keypress(
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

                        render_state.last_cursor = Some(*cursor);

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
