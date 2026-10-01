mod movecommand;
pub use movecommand::Move;
mod system;
pub use system::System;
mod edit;
pub use edit::Edit;
mod mouse;
pub use mouse::MouseCommand;

#[derive(Clone, Copy, Debug)]
pub enum Command {
    Move(Move),
    Edit(Edit),
    System(System),
    Mouse(MouseCommand),
}
