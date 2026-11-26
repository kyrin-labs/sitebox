//! Turn policy: when a model has stopped working, what the harness says next.
//!
//! These decisions used to sit inside the round loop, interleaved with model
//! calls, store writes and event emission, so the only way to test them was a
//! live model and a real build. They are pure: give the policy the request and
//! what has happened so far, and it returns the next move.

use std::collections::HashSet;

/// What the operator asked for, reduced to the two facts the policy needs.
pub struct Request {
    pub wants_change: bool,
    /// Paths the request names; each is a promise the turn must keep.
    pub required: Vec<String>,
    pub max_rounds: usize,
}

impl Request {
    pub fn new(msg: &str, max_rounds: usize) -> Self {
        Self {
            wants_change: wants_change(msg),
            required: named_paths(msg),
            max_rounds,
        }
    }
}

/// What the turn has done so far. The caller updates it after every tool call.
#[derive(Default)]
pub struct State {
    pub mutated: usize,
    pub touched: HashSet<String>,
    pub nudged: bool,
    pub deliverable_nudged: bool,
    pub stuck_nudged: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Nudge {
    /// Asked for a change, produced none, and has not been told yet.
    NoWork,
    /// The request named a path no patch has touched.
    Missing(Vec<String>),
    /// Rounds of reading with nothing written.
    Stuck(String),
}

impl Nudge {
    pub fn message(&self) -> String {
        match self {
            Nudge::NoWork => "You have not changed anything this turn, and the request asks for a \
                 change. Do the work now: put the deliverables in the plan, read only what you \
                 need, and call sitebox_propose_patch. If something truly blocks you, name the \
                 one missing fact — do not ask a vague question."
                .to_string(),
            Nudge::Missing(list) => format!(
                "The request names {}, and no patch has touched it. Create or edit that exact \
                 path now; anything else you noticed goes in one sentence at the end.",
                list.join(", ")
            ),
            Nudge::Stuck(target) => format!(
                "Stop reading. You already have the structure. Make the change now: one \
                 sitebox_propose_patch with create: true for {target} (or edit the named file), \
                 with the full content in `new`. Anything else goes in one sentence at the end."
            ),
        }
    }
}

/// A round is about to start. Six rounds of reading with nothing written means
/// the model is touring, so the policy says the next action outright.
pub fn before_round(state: &mut State, req: &Request, round: usize) -> Option<Nudge> {
    if req.wants_change && state.mutated == 0 && round >= 5 && !state.stuck_nudged {
        state.stuck_nudged = true;
        let target = if req.required.is_empty() {
            "the file the request describes".to_string()
        } else {
            req.required.join(", ")
        };
        return Some(Nudge::Stuck(target));
    }
    None
}

/// The model answered without calling a tool. Decide whether the turn is done
/// or whether it gets one more round with the work restated.
pub fn after_round(state: &mut State, req: &Request, round: usize) -> Option<Nudge> {
    if round + 1 >= req.max_rounds {
        return None;
    }
    if req.wants_change && state.mutated == 0 && !state.nudged {
        state.nudged = true;
        return Some(Nudge::NoWork);
    }
    let missing: Vec<String> = req
        .required
        .iter()
        .filter(|p| !state.touched.contains(p.as_str()))
        .cloned()
        .collect();
    if !missing.is_empty() && !state.deliverable_nudged {
        state.deliverable_nudged = true;
        return Some(Nudge::Missing(missing));
    }
    None
}

/// Paths the request asks for, e.g. `/cases/x.html` or `public/style.css`.
pub fn named_paths(msg: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in msg.split_whitespace() {
        let t = raw.trim_matches(|c: char| {
            c == '`' || c == '"' || c == '\'' || c == '(' || c == ')' || c == ',' || c == ';'
        });
        let lower = t.to_lowercase();
        if (lower.ends_with(".html") || lower.ends_with(".css") || lower.ends_with(".js"))
            && t.contains('/')
        {
            out.push(t.trim_start_matches('/').to_string());
        }
    }
    out
}

/// Does the request ask for a change? Read-only briefs and questions must not
/// trigger the nudge.
pub fn wants_change(msg: &str) -> bool {
    let m = msg.to_lowercase();
    if m.contains("ห้ามแก้")
        || m.contains("อย่าแก้")
        || m.contains("อ่านอย่างเดียว")
        || m.contains("read-only")
        || m.contains("ห้าม patch")
        || m.contains("don't change")
        || m.contains("do not change")
    {
        return false;
    }
    [
        "สร้าง", "แก้", "เพิ่ม", "ทำ", "ปรับ", "เขียน", "อัปเดต", "เปลี่ยน", "ลบ", "ย้าย",
        "build", "create", "add", "make", "fix", "change", "update", "edit", "write",
        "remove", "delete", "move", "refactor", "implement", "redesign",
    ]
    .iter()
    .any(|k| m.contains(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(msg: &str, max: usize) -> Request {
        Request::new(msg, max)
    }

    #[test]
    fn read_only_requests_never_ask_for_work() {
        let r = req("อ่านอย่างเดียว ห้ามแก้", 20);
        assert!(!r.wants_change);
        let mut s = State::default();
        assert_eq!(after_round(&mut s, &r, 0), None);
        assert_eq!(before_round(&mut s, &r, 9), None);
    }

    #[test]
    fn a_change_without_a_write_gets_one_nudge() {
        let r = req("สร้างหน้า /cases/x.html", 20);
        assert!(r.wants_change);
        let mut s = State::default();
        assert_eq!(after_round(&mut s, &r, 0), Some(Nudge::NoWork));
        assert_eq!(after_round(&mut s, &r, 1), Some(Nudge::Missing(vec!["cases/x.html".into()])));
        assert_eq!(after_round(&mut s, &r, 2), None);
    }

    #[test]
    fn a_named_deliverable_is_checked_after_the_work_nudge() {
        let r = req("สร้าง /cases/x.html", 20);
        let mut s = State {
            mutated: 1,
            ..Default::default()
        };
        assert_eq!(after_round(&mut s, &r, 0), Some(Nudge::Missing(vec!["cases/x.html".into()])));
        s.touched.insert("cases/x.html".into());
        assert_eq!(after_round(&mut s, &r, 1), None);
    }

    #[test]
    fn touring_gets_told_at_round_five_once() {
        let r = req("แก้ /index.html", 20);
        let mut s = State::default();
        assert_eq!(before_round(&mut s, &r, 4), None);
        assert_eq!(
            before_round(&mut s, &r, 5),
            Some(Nudge::Stuck("index.html".into()))
        );
        assert_eq!(before_round(&mut s, &r, 6), None);
    }

    #[test]
    fn the_last_round_never_asks_for_more() {
        let r = req("สร้าง /x.html", 6);
        let mut s = State::default();
        assert_eq!(after_round(&mut s, &r, 5), None);
    }

    #[test]
    fn named_paths_ignores_bare_filenames() {
        assert_eq!(named_paths("สร้าง cases/x.html และ index.html"), vec!["cases/x.html"]);
        assert_eq!(named_paths("`/a/b.css`"), vec!["a/b.css"]);
    }
}
