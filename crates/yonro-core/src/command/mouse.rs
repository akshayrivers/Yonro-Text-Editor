use crate::prelude::*;

#[derive(Clone, Copy, Debug)]
pub enum MouseCommand {
    LeftClick(Position),
    LeftDrag(Position),
    LeftRelease(Position),
    ScrollUp(Position),
    ScrollDown(Position),
}
