//! Room templates (hand-drawn text files) and the assembled level.

use glam::{IVec2, Vec2};

use super::tile::{Tile, TileMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoomKind {
    Start,
    Combat,
    Treasure,
    Boss,
    /// Dev/test arenas reachable with `--room`.
    Test,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    North, // -z (top edge of the text)
    South, // +z (bottom edge)
    West,  // -x (left edge)
    East,  // +x (right edge)
}

impl Dir {
    pub fn delta(self) -> IVec2 {
        match self {
            Dir::North => IVec2::new(0, -1),
            Dir::South => IVec2::new(0, 1),
            Dir::West => IVec2::new(-1, 0),
            Dir::East => IVec2::new(1, 0),
        }
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::North => Dir::South,
            Dir::South => Dir::North,
            Dir::West => Dir::East,
            Dir::East => Dir::West,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RoomTemplate {
    pub name: String,
    pub kind: RoomKind,
    pub weight: u32,
    pub waves: u32,
    /// Enemy names this room prefers (empty = any).
    pub enemies: Vec<String>,
    pub w: i32,
    pub h: i32,
    pub tiles: Vec<Tile>,
    /// Door slots on the boundary, with the direction they face out of the room.
    pub doors: Vec<(IVec2, Dir)>,
    pub spawns: Vec<IVec2>,
    pub start: Option<IVec2>,
    pub boss: Option<IVec2>,
    pub chests: Vec<IVec2>,
}

impl RoomTemplate {
    /// Parse a room file:
    ///
    /// ```text
    /// kind: combat
    /// weight: 2
    /// ---
    /// ####D####
    /// #...s...#
    /// ```
    ///
    /// Legend: `#` wall, `.` floor, `D` door slot, `~` pit, `+` pillar, `s` spawn, `t` brazier,
    /// `T` wall torch, `@` start, `B` boss spawn, `c` reward chest, space = outside.
    pub fn parse(name: &str, text: &str) -> Result<RoomTemplate, String> {
        let mut kind = RoomKind::Combat;
        let mut weight = 1;
        let mut waves = 2;
        let mut enemies = Vec::new();
        let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
        let mut line_no = 0;
        for l in lines.by_ref() {
            line_no += 1;
            let t = l.trim();
            if t == "---" {
                break;
            }
            if t.is_empty() || t.starts_with("//") {
                continue;
            }
            let (k, v) = t.split_once(':').ok_or_else(|| format!("line {line_no}: expected 'key: value'"))?;
            let v = v.trim();
            match k.trim() {
                "kind" => {
                    kind = match v {
                        "start" => RoomKind::Start,
                        "combat" => RoomKind::Combat,
                        "treasure" => RoomKind::Treasure,
                        "boss" => RoomKind::Boss,
                        "test" => RoomKind::Test,
                        _ => return Err(format!("line {line_no}: unknown kind '{v}'")),
                    }
                }
                "weight" => weight = v.parse().map_err(|_| format!("line {line_no}: bad weight"))?,
                "waves" => waves = v.parse().map_err(|_| format!("line {line_no}: bad waves"))?,
                "enemies" => enemies = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                other => return Err(format!("line {line_no}: unknown key '{other}'")),
            }
        }
        let body: Vec<&str> = lines.collect();
        let body: Vec<&str> = {
            // Trim blank lines at the end.
            let mut b = body;
            while b.last().is_some_and(|l| l.trim().is_empty()) {
                b.pop();
            }
            b
        };
        if body.is_empty() {
            return Err("room has no map after '---'".into());
        }
        let h = body.len() as i32;
        let w = body.iter().map(|l| l.chars().count()).max().unwrap_or(0) as i32;
        let mut t = RoomTemplate {
            name: name.to_string(),
            kind,
            weight,
            waves,
            enemies,
            w,
            h,
            tiles: vec![Tile::Void; (w * h) as usize],
            doors: Vec::new(),
            spawns: Vec::new(),
            start: None,
            boss: None,
            chests: Vec::new(),
        };
        for (j, l) in body.iter().enumerate() {
            for (i, ch) in l.chars().enumerate() {
                let p = IVec2::new(i as i32, j as i32);
                let tile = match ch {
                    ' ' => Tile::Void,
                    '#' => Tile::Wall,
                    '.' => Tile::Floor,
                    '~' => Tile::Pit,
                    '+' => Tile::Pillar,
                    't' => Tile::Brazier,
                    'T' => Tile::WallTorch,
                    's' => {
                        t.spawns.push(p);
                        Tile::Floor
                    }
                    '@' => {
                        t.start = Some(p);
                        Tile::Floor
                    }
                    'B' => {
                        t.boss = Some(p);
                        Tile::Floor
                    }
                    'c' => {
                        t.chests.push(p);
                        Tile::Chest
                    }
                    'D' => {
                        let dir = if j == 0 {
                            Dir::North
                        } else if j as i32 == h - 1 {
                            Dir::South
                        } else if i == 0 {
                            Dir::West
                        } else if i as i32 == w - 1 {
                            Dir::East
                        } else {
                            return Err(format!("map row {}: door 'D' must be on the room's edge", j + 1));
                        };
                        t.doors.push((p, dir));
                        Tile::Wall // becomes a door only if a corridor connects
                    }
                    other => return Err(format!("map row {}: unknown tile '{other}'", j + 1)),
                };
                let k = (j as i32 * w + i as i32) as usize;
                t.tiles[k] = tile;
            }
        }
        Ok(t)
    }

    pub fn get(&self, i: i32, j: i32) -> Tile {
        if i < 0 || j < 0 || i >= self.w || j >= self.h { Tile::Void } else { self.tiles[(j * self.w + i) as usize] }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomState {
    /// Not entered yet.
    Dormant,
    /// Doors sealed, fighting wave `wave`.
    Active { wave: u32 },
    Cleared,
}

#[derive(Clone, Debug)]
pub struct RoomInst {
    pub template: usize,
    pub kind: RoomKind,
    /// Top-left corner in level tiles.
    pub origin: IVec2,
    pub w: i32,
    pub h: i32,
    /// Door tiles (level coordinates) that were connected to corridors.
    pub doors: Vec<IVec2>,
    pub spawns: Vec<IVec2>,
    pub chests: Vec<IVec2>,
    pub boss: Option<IVec2>,
    pub waves: u32,
    pub enemies: Vec<String>,
    pub state: RoomState,
    /// Order along the critical path (0 = start).
    pub depth: u32,
    /// Countdown used between waves.
    pub timer: f32,
}

impl RoomInst {
    /// Is `p` strictly inside the room (not on its walls)?
    pub fn contains_interior(&self, p: Vec2, margin: f32) -> bool {
        let x0 = self.origin.x as f32 + 1.0 + margin;
        let z0 = self.origin.y as f32 + 1.0 + margin;
        let x1 = (self.origin.x + self.w - 1) as f32 - margin;
        let z1 = (self.origin.y + self.h - 1) as f32 - margin;
        p.x >= x0 && p.x <= x1 && p.y >= z0 && p.y <= z1
    }

    pub fn center(&self) -> Vec2 {
        Vec2::new(self.origin.x as f32 + self.w as f32 / 2.0, self.origin.y as f32 + self.h as f32 / 2.0)
    }
}

#[derive(Clone, Debug)]
pub struct Level {
    pub map: TileMap,
    pub rooms: Vec<RoomInst>,
    pub start: Vec2,
    pub seed: u64,
}

impl Level {
    /// A level made of a single room (used by `--room` and the gallery).
    pub fn single_room(templates: &[RoomTemplate], index: usize) -> Level {
        let t = &templates[index];
        let pad = 2;
        let mut map = TileMap::new(t.w + pad * 2, t.h + pad * 2);
        for j in 0..t.h {
            for i in 0..t.w {
                map.set(i + pad, j + pad, t.get(i, j));
            }
        }
        let origin = IVec2::new(pad, pad);
        let start = t
            .start
            .or_else(|| (0..t.h).flat_map(|j| (0..t.w).map(move |i| IVec2::new(i, j))).find(|p| t.get(p.x, p.y).walkable()))
            .unwrap_or(IVec2::new(t.w / 2, t.h / 2));
        let room = RoomInst {
            template: index,
            kind: t.kind,
            origin,
            w: t.w,
            h: t.h,
            doors: Vec::new(),
            spawns: t.spawns.iter().map(|p| *p + origin).collect(),
            chests: t.chests.iter().map(|p| *p + origin).collect(),
            boss: t.boss.map(|p| p + origin),
            waves: t.waves,
            enemies: t.enemies.clone(),
            state: RoomState::Dormant,
            depth: 1,
            timer: 0.0,
        };
        Level {
            map,
            rooms: vec![room],
            start: Vec2::new((start.x + pad) as f32 + 0.5, (start.y + pad) as f32 + 0.5),
            seed: 0,
        }
    }

    /// Which room (if any) has `p` inside its interior.
    pub fn room_at(&self, p: Vec2) -> Option<usize> {
        self.rooms.iter().position(|r| r.contains_interior(p, 0.0))
    }

    /// Positions of every light-emitting tile.
    pub fn torches(&self) -> Vec<IVec2> {
        let mut v = Vec::new();
        for j in 0..self.map.h {
            for i in 0..self.map.w {
                if matches!(self.map.get(i, j), Tile::WallTorch | Tile::Brazier) {
                    v.push(IVec2::new(i, j));
                }
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_room() {
        let t = RoomTemplate::parse("x", "kind: combat\nweight: 3\n---\n##D##\n#.s.#\nD.@.#\n#####\n").unwrap();
        assert_eq!((t.w, t.h), (5, 4));
        assert_eq!(t.kind, RoomKind::Combat);
        assert_eq!(t.doors.len(), 2);
        assert_eq!(t.doors[0].1, Dir::North);
        assert_eq!(t.doors[1].1, Dir::West);
        assert_eq!(t.spawns, vec![IVec2::new(2, 1)]);
        assert_eq!(t.start, Some(IVec2::new(2, 2)));
    }

    #[test]
    fn door_inside_is_an_error() {
        assert!(RoomTemplate::parse("x", "---\n###\n#D#\n###\n").is_err());
    }
}
