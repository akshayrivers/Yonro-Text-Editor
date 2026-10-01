use crate::prelude::*;
use std::io::Error;

// IMP NOTE : needs to do render in the basis of rect now, not RowIdx or hmmm will need to make another clean renderer
pub trait UIComponent {
    // Marks this UI component as in need of redrawing (or not)
    fn mark_redraw(&mut self, value: bool);
    // Determines if a component needs to be redrawn or not
    fn needs_redraw(&self) -> bool;

    // Updates the size and marks as redraw-needed
    fn resize(&mut self, rect: Rect) {
        self.set_size(rect);
        self.mark_redraw(true);
    }
    // Updates the size. Needs to be implemented by each component.
    fn set_size(&mut self, rect: Rect);

    fn rect(&self) -> Rect;

    // Draw this component if it's visible and in need of redrawing
    // in my design the rect will be owned by the component itself
    fn render(&mut self) {
        if self.needs_redraw() {
            if let Err(err) = self.draw() {
                #[cfg(debug_assertions)]
                {
                    panic!("Could not render component: {err:?}");
                }
                #[cfg(not(debug_assertions))]
                {
                    let _ = err;
                }
            } else {
                self.mark_redraw(false);
            }
        }
    }
    // Method to actually draw the component, must be implemented by each component
    fn draw(&mut self) -> Result<(), Error>;
}
