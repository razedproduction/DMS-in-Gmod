//! The player's tuning of NM's behaviours (docs/adr/0009, ROADMAP M6): a profile's values for messages' params and
//! for the tasks' `taskparams` sets, through NM's own inputs only. Off (`dms_tuning 0`) or with the empty
//! "Playroom" profile, NM gets exactly the Playroom's; a scenario's agent (the bench, `golden`) never gets any.
//!
//! Lua keeps the profiles (`data/dms/profiles/*.json`) and hands the active one over as text (`DMS.SetTuning`):
//! a line per value, `msg <message> <param> <value>` or `tp <task> <set> <param> <value>`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use delirium_data::taskparams::TaskParams;

#[derive(Default)]
struct State {
    /// The active profile's name, while tuning is on.
    name: Option<String>,
    messages: HashMap<String, Vec<(String, String)>>,
    /// The game's taskparams with the profile's values over them; none if it changes none.
    tp: Option<Arc<TaskParams>>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(s.get_or_insert_with(Default::default))
}

/// Tuning on with profile `name`'s lines over the game's taskparams `base`, or off (`None`). The number of values.
pub fn set(profile: Option<(&str, &str)>, base: &TaskParams) -> Result<usize, String> {
    let Some((name, text)) = profile else {
        with(|s| *s = State::default());
        return Ok(0);
    };
    let mut messages: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut tp: Option<TaskParams> = None;
    let mut n = 0;
    for (i, line) in text.lines().map(str::trim).enumerate().filter(|(_, l)| !l.is_empty()) {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.as_slice() {
            ["msg", m, p, v] => {
                let msg = delirium::msg::find(m).ok_or_else(|| format!("line {}: no message {m}", i + 1))?;
                if !msg.params.iter().any(|x| x.name == *p) {
                    return Err(format!("line {}: {m} has no param {p}", i + 1));
                }
                messages.entry(m.to_string()).or_default().push((p.to_string(), v.to_string()));
            }
            ["tp", task, set, p, v] => {
                let v: f32 = v.parse().map_err(|_| format!("line {}: {v} isn't a number", i + 1))?;
                let t = tp.get_or_insert_with(|| base.clone());
                t.0.entry(task.to_string()).or_default().entry(set.to_string()).or_default().insert(p.to_string(), vec![v]);
            }
            _ => return Err(format!("line {}: want `msg <message> <param> <value>` or `tp <task> <set> <param> <value>`", i + 1)),
        }
        n += 1;
    }
    with(|s| *s = State { name: Some(name.to_string()), messages, tp: tp.map(Arc::new) });
    Ok(n)
}

/// The profile's values for `message`'s params (none while tuning is off).
pub fn params(message: &str) -> Vec<(String, String)> {
    with(|s| s.messages.get(message).cloned().unwrap_or_default())
}

/// The tuned taskparams, if the profile changes any.
pub fn taskparams() -> Option<Arc<TaskParams>> {
    with(|s| s.tp.clone())
}

/// The active profile's name, while tuning is on.
pub fn active() -> Option<String> {
    with(|s| s.name.clone())
}
