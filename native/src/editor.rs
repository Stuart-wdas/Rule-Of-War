//! Sparse, stroke-based undo/redo. Revisions always increase to invalidate meshes.
use crate::terrain::{Cell, Terrain};
use bevy::prelude::Resource;
use std::collections::{BTreeMap, VecDeque};
type Change = (String, Option<Cell>, Option<Cell>);
#[derive(Resource, Default)]
pub struct History {
    stroke: Option<BTreeMap<String, Option<Cell>>>,
    undo: VecDeque<Vec<Change>>,
    redo: Vec<Vec<Change>>,
}
impl History {
    pub fn paint(
        &mut self,
        terrain: &mut Terrain,
        x: f32,
        z: f32,
        radius: i32,
        delta: f32,
        water: bool,
    ) {
        let stroke = self.stroke.get_or_insert_with(BTreeMap::new);
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dz * dz > radius * radius {
                    continue;
                }
                let (cx, cz) = (x.round() as i32 + dx, z.round() as i32 + dz);
                if cx < 0 || cz < 0 || cx >= terrain.cols || cz >= terrain.rows {
                    continue;
                }
                let key = format!("{cx},{cz}");
                stroke
                    .entry(key.clone())
                    .or_insert_with(|| terrain.edits.get(&key).copied());
            }
        }
        terrain.brush(x, z, radius, delta, water);
    }
    pub fn finish(&mut self, terrain: &Terrain) {
        if let Some(stroke) = self.stroke.take() {
            let changes: Vec<_> = stroke
                .into_iter()
                .filter_map(|(key, before)| {
                    let after = terrain.edits.get(&key).copied();
                    (before != after).then_some((key, before, after))
                })
                .collect();
            if changes.is_empty() {
                return;
            }
            self.undo.push_back(changes);
            self.redo.clear();
            // Bound retained history by total changed cells, including long strokes.
            while self.undo.len() > 1
                && (self.undo.len() > 32 || self.undo.iter().map(Vec::len).sum::<usize>() > 250_000)
            {
                self.undo.pop_front();
            }
        }
    }
    pub fn undo(&mut self, terrain: &mut Terrain) {
        self.finish(terrain);
        if let Some(changes) = self.undo.pop_back() {
            apply(terrain, &changes, false);
            self.redo.push(changes);
        }
    }
    pub fn redo(&mut self, terrain: &mut Terrain) {
        self.finish(terrain);
        if let Some(changes) = self.redo.pop() {
            apply(terrain, &changes, true);
            self.undo.push_back(changes);
        }
    }
}
fn apply(terrain: &mut Terrain, changes: &[Change], forward: bool) {
    for (key, before, after) in changes {
        if let Some(cell) = if forward { after } else { before } {
            terrain.edits.insert(key.clone(), *cell);
        } else {
            terrain.edits.remove(key);
        }
    }
    terrain.revision += 1;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn undo_redo_preserve_prior_edits_and_invalidate_rendering() {
        let mut t = Terrain::default();
        let mut h = History::default();
        h.paint(&mut t, 30., 30., 2, 0.1, false);
        h.paint(&mut t, 31., 30., 2, 0.1, false);
        h.finish(&t);
        let first = t.edits.clone();
        h.paint(&mut t, 30., 30., 1, -0.1, false);
        h.finish(&t);
        let second = t.edits.clone();
        let revision = t.revision;
        h.undo(&mut t);
        assert_eq!(t.edits, first);
        assert!(t.revision > revision);
        h.undo(&mut t);
        assert!(t.edits.is_empty());
        h.redo(&mut t);
        assert_eq!(t.edits, first);
        h.redo(&mut t);
        assert_eq!(t.edits, second);
        h.undo(&mut t);
        h.paint(&mut t, 50., 50., 1, 0.1, false);
        h.finish(&t);
        assert!(h.redo.is_empty());
    }
}
