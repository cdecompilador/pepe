use std::path::PathBuf;

use crossterm::{execute, terminal};
use crossterm::event::*;

mod input;
mod render;
mod text;

use crate::input::{Cursor, CursorState, process_keypress};
use crate::render::{RenderState, refresh_screen};
use crate::text::Document;

/// Wrapper around Result
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The state of the full editor itself
pub struct EditorState {
    /// Lines of the document
    doc_lines: usize,

    /// Used to tell if the application should close
    running: bool,

    /// Size of the terminal, updated always
    rows: usize,
    columns: usize,
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
