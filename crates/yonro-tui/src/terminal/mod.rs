// This will be turned into the TUI render layer with support of a render pipeline and hooks
// Also the need to handle custom events is also there, so eventualy need to be smart about it
mod attribute;
use crate::prelude::*;
use yonro_core::AnnotatedString;
use attribute::Attribute;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::style::{Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, size, Clear, ClearType, DisableLineWrap, EnableLineWrap,
    EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
};
use crossterm::{queue, Command};
use std::io::{stdout, Error, Write};
pub struct Terminal {}

impl Terminal {
    pub fn terminate() -> Result<(), Error> {
        Self::leave_alternate_screen()?;
        Self::enable_line_wrap()?;
        Self::show_caret()?;
        Self::execute()?;
        Self::disable_mouse_capture()?;
        disable_raw_mode()?;
        Ok(())
    }
    pub fn execute() -> Result<(), Error> {
        stdout().flush()?;
        Ok(())
    }
    pub fn initialize() -> Result<(), Error> {
        enable_raw_mode()?;
        Self::enable_mouse_capture()?;
        Self::enter_alternate_screen()?;
        Self::disable_line_wrap()?;
        Self::clear_screen()?;
        Self::execute()?;
        Ok(())
    }
    fn queue_command<T: Command>(command: T) -> Result<(), Error> {
        queue!(stdout(), command)?;
        Ok(())
    }
    pub fn enable_mouse_capture() -> Result<(), Error> {
        Self::queue_command(EnableMouseCapture)?;
        Ok(())
    }

    pub fn disable_mouse_capture() -> Result<(), Error> {
        Self::queue_command(DisableMouseCapture)?;
        Ok(())
    }
    pub fn clear_screen() -> Result<(), Error> {
        Self::queue_command(Clear(ClearType::All))?;
        Ok(())
    }
    // pub fn clear_line() -> Result<(), Error> {
    //     Self::queue_command(Clear(ClearType::CurrentLine))?;
    //     Ok(())
    // }

    pub fn clear_rect_line(rect: Rect, row: RowIdx) -> Result<(), Error> {
        Self::move_caret_to(Position {
            row,
            col: rect.position.col,
        })?;

        let blank = " ".repeat(rect.size.width);

        Self::print(&blank)?;

        Ok(())
    }

    pub fn move_caret_to(position: Position) -> Result<(), std::io::Error> {
        #[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
        Self::queue_command(MoveTo(position.col as u16, position.row as u16))?;

        Ok(())
    }
    pub fn hide_caret() -> Result<(), Error> {
        Self::queue_command(Hide)?;
        Ok(())
    }
    pub fn show_caret() -> Result<(), Error> {
        Self::queue_command(Show)?;
        Ok(())
    }
    pub fn disable_line_wrap() -> Result<(), Error> {
        Self::queue_command(DisableLineWrap)?;
        Ok(())
    }
    pub fn enable_line_wrap() -> Result<(), Error> {
        Self::queue_command(EnableLineWrap)?;
        Ok(())
    }
    pub fn set_title(title: &str) -> Result<(), Error> {
        Self::queue_command(SetTitle(title))?;
        Ok(())
    }
    pub fn print(string: &str) -> Result<(), Error> {
        Self::queue_command(Print(string))?;
        Ok(())
    }
    pub fn size() -> Result<Size, Error> {
        let (width_u16, height_u16) = size()?;

        #[allow(clippy::as_conversions)]
        let height = height_u16 as usize;

        #[allow(clippy::as_conversions)]
        let width = width_u16 as usize;

        Ok(Size { height, width })
    }

    pub fn enter_alternate_screen() -> Result<(), Error> {
        Self::queue_command(EnterAlternateScreen)?;
        Ok(())
    }

    pub fn leave_alternate_screen() -> Result<(), Error> {
        Self::queue_command(LeaveAlternateScreen)?;
        Ok(())
    }

    pub fn print_at(position: Position, text: &str) -> Result<(), Error> {
        Self::move_caret_to(position)?;
        Self::print(text)?;

        Ok(())
    }

    pub fn print_annotated_at(
        position: Position,
        annotated_string: &AnnotatedString,
    ) -> Result<(), Error> {
        Self::move_caret_to(position)?;

        annotated_string
            .into_iter()
            .try_for_each(|part| -> Result<(), Error> {
                if let Some(annotation_type) = part.annotation_type {
                    let attribute: Attribute = annotation_type.into();
                    Self::set_attribute(&attribute)?;
                }

                Self::print(part.string)?;

                Self::reset_color()?;

                Ok(())
            })?;

        Ok(())
    }

    // pub fn print_row(row: RowIdx, line_text: &str) -> Result<(), Error> {
    //     Self::move_caret_to(Position { row, col: 0 })?;

    //     Self::clear_line()?;
    //     Self::print(line_text)?;

    //     Ok(())
    // }

    // pub fn print_annotated_row(
    //     row: RowIdx,
    //     annotated_string: &AnnotatedString,
    // ) -> Result<(), Error> {
    //     Self::move_caret_to(Position { row, col: 0 })?;

    //     Self::clear_line()?;

    //     annotated_string
    //         .into_iter()
    //         .try_for_each(|part| -> Result<(), Error> {
    //             if let Some(annotation_type) = part.annotation_type {
    //                 let attribute: Attribute = annotation_type.into();
    //                 Self::set_attribute(&attribute)?;
    //             }

    //             Self::print(part.string)?;

    //             Self::reset_color()?;

    //             Ok(())
    //         })?;

    //     Ok(())
    // }

    pub fn print_rect(rect: Rect, row_offset: usize, text: &str) -> Result<(), Error> {
        let row = rect.position.row.saturating_add(row_offset);

        Self::clear_rect_line(rect, row)?;

        Self::print_at(
            Position {
                row,
                col: rect.position.col,
            },
            text,
        )?;

        Ok(())
    }

    pub fn print_annotated_rect(
        rect: Rect,
        row_offset: usize,
        annotated_string: &AnnotatedString,
    ) -> Result<(), Error> {
        let row = rect.position.row.saturating_add(row_offset);

        Self::clear_rect_line(rect, row)?;

        Self::print_annotated_at(
            Position {
                row,
                col: rect.position.col,
            },
            annotated_string,
        )?;

        Ok(())
    }

    pub fn set_attribute(attribute: &Attribute) -> Result<(), Error> {
        if let Some(foreground_color) = attribute.foreground {
            Self::queue_command(SetForegroundColor(foreground_color))?;
        }

        if let Some(background_color) = attribute.background {
            Self::queue_command(SetBackgroundColor(background_color))?;
        }

        Ok(())
    }

    pub fn reset_color() -> Result<(), Error> {
        Self::queue_command(ResetColor)?;
        Ok(())
    }

    // pub fn print_inverted_row(row: RowIdx, line_text: &str) -> Result<(), Error> {
    //     let width = Self::size()?.width;

    //     Self::print_row(row, &format!("{Reverse}{line_text:width$.width$}{Reset}"))
    // }
    pub fn draw_border(rect: Rect) -> Result<(), Error> {
        let Position { row, col } = rect.position;

        let Size { height, width } = rect.size;

        if width < 2 || height < 2 {
            return Ok(());
        }

        // Top
        Self::print_at(
            Position { row, col },
            &format!("┌{}┐", "─".repeat(width.saturating_sub(2))),
        )?;

        // Sides
        for current_row in row + 1..row + height.saturating_sub(1) {
            Self::print_at(
                Position {
                    row: current_row,
                    col,
                },
                "│",
            )?;

            Self::print_at(
                Position {
                    row: current_row,
                    col: col + width.saturating_sub(1),
                },
                "│",
            )?;
        }

        // Bottom
        Self::print_at(
            Position {
                row: row + height.saturating_sub(1),
                col,
            },
            &format!("└{}┘", "─".repeat(width.saturating_sub(2))),
        )?;

        Ok(())
    }
    pub fn wait_for_event() -> Result<yonro_core::events::EditorEvent, std::io::Error> {
        let event = crossterm::event::read()?;
        Ok(crate::input::from_crossterm(event))
    }
}
