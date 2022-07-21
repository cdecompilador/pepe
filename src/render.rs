//! All primitives related to rendering to the screen, the phylosophy is to
//! only redraw what needs to be readrawn and NOT in loop, event model

use std::io::{Write, Stdout};

use crossterm::{queue, execute, terminal};
use crossterm::style::{Print, PrintStyledContent, Color, Stylize};

use crate::{Result, EditorState};
use crate::input::{Cursor, CursorState};
use crate::text::Document;

/// Settings used to do the rendering on a optimized way
pub struct RenderState {
    /// Row that needs to be repainted
    pub modif_row: Option<usize>,

    /// If all the terminal needs to be repainted
    pub modif_all: bool,

    /// The position of the cursor, if `Some`, used to update the cursor 
    pub last_cursor: Option<Cursor>,

    /// If the status bar needs to be repainted
    pub modif_status: bool
}

/// Update (if needed) the elements that need to be updated on the screen
pub fn refresh_screen(
    stdout: &mut Stdout,
    document: &Option<Document>,
    cursor: &Cursor,
    EditorState { rows, columns, .. }: &EditorState,
    CursorState { scroll_y, .. }: &CursorState,
    RenderState { 
        modif_row, 
        modif_all, 
        last_cursor, 
        modif_status 
    }: &RenderState
) -> Result<()> {
    // Check if the status bar needs to be repainted
    if *modif_status {
        queue!(stdout,
            crossterm::cursor::SavePosition,
            crossterm::cursor::MoveTo(0, *rows as u16),
            PrintStyledContent(
                render_status_bar(
                    document, 
                    cursor, 
                    *columns, 
                    *scroll_y)
                .with(Color::Black)
                .on(Color::White)),
            crossterm::cursor::RestorePosition)?;
    }

    // Re-draw all the rows when modif_all
    if *modif_all {
        // Print the document lines
        if let Some(doc) = document {
            // Hide the cursor
            queue!(stdout, 
                crossterm::cursor::MoveTo(
                    cursor.column as u16 + 4, cursor.row as u16),
                crossterm::cursor::SavePosition,
                crossterm::cursor::Hide)?;

            for row in 0..*rows as u16 {
                // Clear this line
                queue!(stdout, 
                    crossterm::cursor::MoveToRow(row),
                    terminal::Clear(terminal::ClearType::CurrentLine))?;

                let idx = row as usize + scroll_y;
                if let Some(line) = &doc.inner_lines.get(idx) {
                    // Print the document
                    queue!(stdout,
                        crossterm::cursor::MoveTo(0, row),
                        PrintStyledContent(
                            format!("{:3} ", idx)
                                .with(Color::Yellow)),
                        Print(line))?;
                } else {
                    queue!(stdout,
                        crossterm::cursor::MoveTo(0, row),
                        Print("~ "
                            .with(Color::Yellow)))?;
                }
            }

            // Show again the cursor
            queue!(stdout, 
                crossterm::cursor::RestorePosition,
                crossterm::cursor::Show)?;

        // No file loaded so this is the first print of logo screen
        } else {
            for row in 0..(*rows - 1) as u16 {
                // Clear this line
                queue!(stdout, 
                    crossterm::cursor::MoveToRow(row),
                    terminal::Clear(terminal::ClearType::CurrentLine))?;

                // Print the intro (no document opened)
                if row == *rows as u16 / 3 {
                    let msg = "Pepe editor -- version 0.0.1"
                        .with(Color::Blue);
                    let msg_start = *columns / 2 - msg.content().len() / 2;

                    queue!(stdout,
                        crossterm::cursor::MoveTo(0, row),
                        Print("~ "
                            .with(Color::Yellow)),
                        crossterm::cursor::MoveTo(msg_start as u16, row),
                        PrintStyledContent(msg))?;
                } else {
                    queue!(stdout,
                        crossterm::cursor::MoveTo(0, row),
                        Print("~ "
                            .with(Color::Yellow)))?;
                }
            }
        }
    } else if let Some(row) = modif_row {
        let idx = row + scroll_y;
        let line = &document.as_ref().unwrap().inner_lines[idx];

        queue!(stdout,
            crossterm::cursor::SavePosition,
            crossterm::cursor::MoveTo(0, *row as u16),
            PrintStyledContent(
                format!("{:3} ", idx)
                    .with(Color::Yellow)),
            Print(line),
            crossterm::cursor::RestorePosition)?
    }

    if last_cursor.is_some() && *modif_all == false {
        let last_cursor = last_cursor.unwrap();

        queue!(stdout, 
            crossterm::cursor::Hide,
            crossterm::cursor::MoveTo(
                cursor.column as u16 + 4, cursor.row as u16),
            crossterm::cursor::Show)?;
    }

    // Send all the draw commands at once
    stdout.flush()?;

    Ok(())
}

/// Print the status bar
///
/// TODO: Modifications in-place of the `status_msg` might improve perf
fn render_status_bar(
    document: &Option<Document>, 
    cursor: &Cursor,
    columns: usize,
    scroll_y: usize
) -> String {
    let mut status_msg = String::with_capacity(columns);
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

    status_msg
}
