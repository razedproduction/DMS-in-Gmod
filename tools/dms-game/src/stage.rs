//! The Playroom's stages (`delirium-room` `world::stage`, `--stage <name>`, v0.2.2): the boxes each puts on the room's
//! floor (no pool) and where Homer starts. The same numbers, for the stages map (`tools/dms-map`) and the bench
//! (ROADMAP M5). Only the stages of fixed boxes are here (no bridge, platforms, cab or street traffic).

/// What a box is made of: materials.dat's BRICK_WALL or CONCRETE row, as the Playroom picks them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Block {
    Brick,
    Concrete,
}

/// A stage: its boxes (centre, half sizes, m) and Homer's start (his lowest point there, the pelvis above it).
pub struct Stage {
    pub name: &'static str,
    pub boxes: Vec<([f32; 3], [f32; 3], Block)>,
    pub start: [f32; 3],
}

/// The stages there are.
pub const NAMES: [&str; 8] = ["bigstairs", "stairsdown", "rooftop", "highledge", "lowwall", "kerb", "rail", "open"];

/// A stage by name, as `world::stage` has it.
pub fn stage(name: &str) -> Option<Stage> {
    use Block::{Brick, Concrete};
    let b = |c: [f32; 3], h: [f32; 3], r| (c, h, r);
    let (boxes, start) = match name {
        "bigstairs" => {
            let mut v = vec![b([0.0, -0.45, 3.0], [1.5, 1.05, 3.0], Concrete)];
            v.extend([-1.6, 1.6].map(|x| b([x, -0.45, 3.5], [0.1, 1.05, 3.5], Brick)));
            for i in 0..39 {
                let (top, y) = (6.0 - 0.15 * (i + 1) as f32, 0.75 + 0.3 * i as f32);
                v.push(b([0.0, y, top * 0.5], [1.5, 0.15, top * 0.5], Concrete));
                v.extend([-1.6, 1.6].map(|x| b([x, y, (top + 1.0) * 0.5], [0.1, 0.15, (top + 1.0) * 0.5], Brick)));
            }
            (v, [0.0, 0.0, 6.0])
        }
        "stairsdown" => {
            let s = stage("bigstairs")?;
            (s.boxes.into_iter().map(|(c, h, r)| ([c[0], -c[1], c[2]], h, r)).collect(), s.start)
        }
        "rooftop" => (vec![b([0.0, -2.25, 2.0], [3.0, 2.75, 2.0], Concrete)], [0.0, 0.0, 4.0]),
        "highledge" => (vec![b([0.0, 4.5, 6.0], [5.0, 5.0, 6.0], Concrete)], [0.0, 0.0, 12.0]),
        "lowwall" => (vec![b([0.0, 0.45, 0.45], [4.0, 0.15, 0.45], Brick)], [0.0; 3]),
        "kerb" => (vec![b([0.0, 0.45, 0.22], [4.0, 0.15, 0.22], Brick)], [0.0; 3]),
        "rail" => (
            vec![b([0.0, -0.5, 1.18], [1.5, 0.03, 0.03], Concrete), b([-1.5, -0.5, 0.6], [0.03, 0.03, 0.6], Concrete), b([1.5, -0.5, 0.6], [0.03, 0.03, 0.6], Concrete)],
            [0.0; 3],
        ),
        "open" => (vec![], [0.0; 3]),
        _ => return None,
    };
    Some(Stage { name: NAMES.iter().copied().find(|n| *n == name)?, boxes, start })
}
