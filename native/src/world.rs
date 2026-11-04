//! Strategic rules ported from simulation/world-state.js.
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sector {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub name: String,
    pub biome: String,
    pub terrain_seed: String,
    pub owner: Option<i32>,
    pub objective: String,
    pub supply: u8,
    pub contested: bool,
    pub battle_id: Option<String>,
    #[serde(default)]
    pub modifications: Vec<serde_json::Value>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct World {
    pub schema: u32,
    pub id: String,
    pub seed: String,
    pub turn: u64,
    pub width: i32,
    pub height: i32,
    pub sectors: Vec<Sector>,
    #[serde(default)]
    pub events: Vec<serde_json::Value>,
    #[serde(default)]
    pub armies: Vec<serde_json::Value>,
    #[serde(default)]
    pub factions: Vec<serde_json::Value>,
}
impl Default for World {
    fn default() -> Self {
        Self::new("rule-of-war")
    }
}
impl World {
    pub fn new(seed: &str) -> Self {
        let mut sectors = vec![];
        for y in 0..4 {
            for x in 0..10 {
                let owner = if y == 2 && x == 0 {
                    Some(0)
                } else if y == 2 && x == 9 {
                    Some(1)
                } else {
                    None
                };
                sectors.push(Sector {
                    id: format!("sector-{x}-{y}"),
                    x,
                    y,
                    name: format!("SECTOR {:02}", y * 10 + x + 1),
                    biome: ["plains", "forest", "highlands", "coast"]
                        [((x * 3 + y * 5) % 4) as usize]
                        .into(),
                    terrain_seed: format!("{seed}:{x}:{y}"),
                    owner,
                    objective: if (x + y) % 3 == 0 {
                        "settlement"
                    } else {
                        "command-post"
                    }
                    .into(),
                    supply: if owner.is_some() { 1 } else { 0 },
                    contested: false,
                    battle_id: None,
                    modifications: vec![],
                });
            }
        }
        Self {
            schema: 3,
            id: format!("world-{seed}"),
            seed: seed.into(),
            turn: 1,
            width: 10,
            height: 4,
            sectors,
            events: vec![],
            armies: vec![],
            factions: vec![
                serde_json::json!({"id":0,"name":"FIELD COMMAND","color":"#5fa87a"}),
                serde_json::json!({"id":1,"name":"FRONT 2","color":"#b55f55"}),
            ],
        }
    }
    pub fn neighbors(&self, index: usize) -> Vec<usize> {
        let s = &self.sectors[index];
        let d = if s.y % 2 == 0 { -1 } else { 1 };
        [
            (s.x - 1, s.y),
            (s.x + 1, s.y),
            (s.x, s.y - 1),
            (s.x, s.y + 1),
            (s.x + d, s.y - 1),
            (s.x + d, s.y + 1),
        ]
        .iter()
        .filter_map(|&(x, y)| self.sectors.iter().position(|s| s.x == x && s.y == y))
        .collect()
    }
    pub fn can_contest(&self, index: usize, owner: i32) -> bool {
        index < self.sectors.len()
            && self.sectors[index].owner != Some(owner)
            && self
                .neighbors(index)
                .iter()
                .any(|i| self.sectors[*i].owner == Some(owner))
    }
    pub fn supply(&mut self) {
        for s in &mut self.sectors {
            s.supply = 0;
        }
        for owner in [0, 1] {
            let edge = if owner == 0 { 0 } else { self.width - 1 };
            let mut queue: Vec<usize> = self
                .sectors
                .iter()
                .enumerate()
                .filter(|(_, s)| s.owner == Some(owner) && s.x == edge)
                .map(|(i, _)| i)
                .collect();
            let mut cursor = 0;
            while cursor < queue.len() {
                let i = queue[cursor];
                self.sectors[i].supply = 1;
                for n in self.neighbors(i) {
                    if self.sectors[n].owner == Some(owner) && !queue.contains(&n) {
                        queue.push(n);
                    }
                }
                cursor += 1;
            }
        }
    }
    pub fn outcome(&mut self, index: usize, winner: i32) {
        if index >= self.sectors.len() || !(0..=1).contains(&winner) {
            return;
        }
        let old = self.sectors[index].owner;
        let s = &mut self.sectors[index];
        s.owner = Some(winner);
        s.contested = false;
        s.battle_id = None;
        self.events.push(serde_json::json!({"type":"sector-captured","turn":self.turn,"sectorId":s.id,"previousOwner":old,"owner":winner}));
        self.supply();
        self.turn += 1;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strategic_rules_match_web_fixture() {
        let expected: World =
            serde_json::from_str(include_str!("../tests/web-world.json")).unwrap();
        let mut actual = World::default();
        actual.outcome(21, 0);
        assert_eq!(
            serde_json::to_value(actual.sectors).unwrap(),
            serde_json::to_value(expected.sectors).unwrap()
        );
        assert_eq!(actual.events, expected.events);
    }
    #[test]
    fn contested_adjacency_and_supply() {
        let mut w = World::default();
        assert!(w.can_contest(21, 0));
        assert!(!w.can_contest(25, 0));
        w.outcome(21, 0);
        assert_eq!(w.sectors[21].supply, 1);
        w.outcome(25, 0);
        assert_eq!(w.sectors[25].supply, 0);
        assert_eq!(w.turn, 3);
    }
}
