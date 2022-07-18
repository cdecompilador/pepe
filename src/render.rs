use std::io::{Write, Stdout};

use crossterm::{queue, execute, terminal};
use crossterm::style::{Print, PrintStyledContent, Color, Stylize};

use crate::{Result, EditorState};
use crate::input::{Cursor, CursorState};
use crate::text::Document;

/// Settings used to do the rendering on a optimized way
pub struct RenderState {
    /// Columns that needs to be repainted
    pub modif_column: Option<usize>,

    /// If all the terminal needs to be repainted
    pub modif_all: bool,

    /// The position of the cursor, if `Some`, used to update the cursor 
    pub last_cursor: Option<Cursor>
}

/// Update (if needed) the elements that need to be updated on the screen
pub fn refresh_screen(
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
