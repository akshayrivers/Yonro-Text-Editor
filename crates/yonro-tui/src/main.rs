#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::print_stdout,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::integer_division
)]

use yonro_tui::Editor;

fn main() {
    Editor::new().unwrap().run();
}
