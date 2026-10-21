//! `dms_selftest`: Homer's agent in LEAD on the Playroom's floor, `bodyBalance` (the Playroom's "balance") for
//! a couple of seconds — he has to stay up (ROADMAP M2). No game physics is involved: NM moves its own body.

use crate::data::{self, Data};
use crate::nm::{self, Mode, Value};

/// What came of it.
pub struct Report {
    pub steps: usize,
    pub pelvis_start: f32,
    pub pelvis_min: f32,
    pub pelvis_end: f32,
    pub head_start: f32,
    pub head_end: f32,
    pub balancing: bool,
    pub fallen: bool,
    pub passed: bool,
    pub verdict: String,
}

/// Runs `seconds` of NM steps (1/60 s).
pub fn run(d: &Data, seconds: f32) -> Result<Report, String> {
    if !(seconds > 0.0 && seconds <= 600.0) {
        return Err("seconds: 0 to 600".into());
    }
    let head = (0..nm::PARTS).find(|&i| nm::part_name(i).is_ok_and(|n| n == "Head")).ok_or("no part Head")?;
    let world = data::floor_world(d)?;
    // As the Playroom's: standing at the room's start (the origin), facing as built, seed 0.3.
    let mut a = nm::Agent::new(&d.build, [0.0; 3], 0.0, nm::PLAYROOM_SEED)?;
    a.set_mode(Mode::Lead)?;
    a.send(&world, "bodyBalance", &[("start", Value::Bool(true))])?;
    let start = a.part_transforms()?;
    let (pelvis_start, head_start) = (start[0].0[2], start[head].0[2]);
    let (mut pelvis_min, mut finite) = (pelvis_start, true);
    let steps = (seconds / nm::DT).round() as usize;
    let mut now = start;
    for _ in 0..steps {
        a.step(&world, nm::DT)?;
        now = a.part_transforms()?;
        finite &= now.iter().all(|(p, r)| p.iter().chain(r).all(|x| x.is_finite()));
        if !finite {
            break;
        }
        pelvis_min = pelvis_min.min(now[0].0[2]);
    }
    let (pelvis_end, head_end) = (now[0].0[2], now[head].0[2]);
    let (balancing, fallen) = (a.is_balancing()?, a.has_fallen()?);
    // Standing: still balancing, not fallen, the pelvis and head close to their standing heights.
    let checks = [
        (finite, "NaN/Inf in the body"),
        (!fallen, "NM says he has fallen"),
        (balancing, "he isn't balancing"),
        (pelvis_min > 0.75 * pelvis_start, "the pelvis sank below 3/4 of its height"),
        (head_end > 0.85 * head_start, "the head dropped below 85% of its height"),
    ];
    let failed: Vec<&str> = checks.iter().filter(|c| !c.0).map(|c| c.1).collect();
    let passed = failed.is_empty();
    Ok(Report {
        steps,
        pelvis_start,
        pelvis_min,
        pelvis_end,
        head_start,
        head_end,
        balancing,
        fallen,
        passed,
        verdict: if passed { "PASS: he stands".into() } else { format!("FAIL: {}", failed.join("; ")) },
    })
}
