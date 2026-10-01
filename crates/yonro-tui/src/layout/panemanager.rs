use crate::layout::{Pane, PaneContent};
use crate::prelude::Rect;
use std::collections::HashMap;

#[derive(Default)]
pub struct PaneManager {
    panes: HashMap<usize, Pane>,
    active_pane: usize,
    next_pane_id: usize,
}

impl PaneManager {
    pub fn new(mut initial_pane: Pane) -> Self {
        let initial_pane_id = initial_pane.pane_id;
        initial_pane.set_content_active(true);

        let mut panes = HashMap::new();
        panes.insert(initial_pane_id, initial_pane);

        Self {
            panes,
            active_pane: initial_pane_id,
            next_pane_id: initial_pane_id + 1,
        }
    }

    pub fn create_pane(&mut self, content: PaneContent) -> usize {
        let pane_id = self.next_pane_id;
        self.next_pane_id += 1;

        let pane = Pane {
            pane_id,
            content,
            active: false,
            is_floating: false,
            z_index: 1, // at 0 there will be tiles
            is_minimized: false,
            rect: Rect::default(),
        };

        self.panes.insert(pane_id, pane);
        pane_id
    }

    pub fn create_floating_pane(&mut self, content: PaneContent, z_index: usize) -> usize {
        let pane_id = self.next_pane_id;
        self.next_pane_id += 1;

        let pane = Pane {
            pane_id,
            content,
            active: false,
            is_floating: true,
            z_index,
            is_minimized: false,
            rect: Rect::default(),
        };

        self.panes.insert(pane_id, pane);
        pane_id
    }

    pub fn get_pane(&self, pane_id: usize) -> Option<&Pane> {
        self.panes.get(&pane_id)
    }

    pub fn get_pane_mut(&mut self, pane_id: usize) -> Option<&mut Pane> {
        self.panes.get_mut(&pane_id)
    }

    pub fn active_pane(&self) -> Option<&Pane> {
        self.panes.get(&self.active_pane)
    }

    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.get_mut(&self.active_pane)
    }

    pub fn set_active_pane(&mut self, pane_id: usize) {
        if !self.panes.contains_key(&pane_id) {
            return;
        }
        if let Some(current_active) = self.panes.get_mut(&self.active_pane) {
            current_active.active = false;
            current_active.set_content_active(false);
        }

        if let Some(new_active) = self.panes.get_mut(&pane_id) {
            new_active.active = true;
            new_active.set_content_active(true);
        }

        self.active_pane = pane_id;
    }

    pub fn remove_pane(&mut self, pane_id: usize) -> Option<Pane> {
        self.panes.remove(&pane_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Pane> {
        self.panes.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Pane> {
        self.panes.values_mut()
    }

    pub fn get_floating_panes_sorted(&self) -> Vec<&Pane> {
        let mut floating: Vec<&Pane> = self.panes.values().filter(|p| p.is_floating).collect();
        floating.sort_by_key(|p| p.z_index);
        floating
    }

    pub fn get_floating_panes_sorted_mut(&mut self) -> Vec<&mut Pane> {
        let mut floating: Vec<&mut Pane> =
            self.panes.values_mut().filter(|p| p.is_floating).collect();
        floating.sort_by_key(|p| p.z_index);
        floating
    }

    pub fn bring_to_front(&mut self, pane_id: usize) {
        if !self.panes.get(&pane_id).map_or(false, |p| p.is_floating) {
            return;
        }

        let Some(top_id) = self
            .panes
            .values()
            .filter(|p| p.is_floating)
            .max_by_key(|p| p.z_index)
            .map(|p| p.pane_id)
        else {
            return;
        };

        let top_z = self.panes[&top_id].z_index;
        let target_z = self.panes[&pane_id].z_index;

        if top_z == target_z {
            // tie — swap would be a no-op, so bump instead
            self.panes.get_mut(&pane_id).unwrap().z_index = top_z + 1;
        } else {
            self.panes.get_mut(&top_id).unwrap().z_index = target_z;
            self.panes.get_mut(&pane_id).unwrap().z_index = top_z;
        }
    }
}
