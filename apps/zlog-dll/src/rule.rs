use std::path::Path;
use std::time::Duration;

use zlog_cfg::file::load_cfg;
use zlog_cfg::user::UserContest;
use zlog_core::{rescore, Contest, ContestMode, Log, Qso};

pub const PATIENCE: Duration = Duration::from_secs(10);

pub struct Rule {
    contest: UserContest,
    log: Log,
    scored: Option<(Log, u64)>,
}

fn key(q: &Qso) -> Qso {
    let mut q = q.clone();
    q.dupe = false;
    q.points = 0;
    q.multi1.clear();
    q.multi2.clear();
    q.multi3.clear();
    q.new_multi1 = false;
    q.new_multi2 = false;
    q.new_multi3 = false;
    q
}

impl Rule {
    pub fn open(cfg: &Path) -> Result<Rule, String> {
        zlog_lua::install();
        zlog_cfg::script::host::patience(PATIENCE);
        let (def, _) = load_cfg(cfg)?;
        Ok(Rule {
            contest: def.build_rules(ContestMode::All),
            log: Log::default(),
            scored: None,
        })
    }

    pub fn contest(&self) -> &dyn Contest {
        &self.contest
    }

    fn find(&self, q: &Qso) -> Option<usize> {
        if q.reserve3 != 0 {
            return self.log.qsos.iter().position(|x| x.reserve3 == q.reserve3);
        }
        let k = key(q);
        self.log.qsos.iter().position(|x| *x == k)
    }

    pub fn insert(&mut self, q: &Qso) {
        self.log.qsos.push(key(q));
        self.scored = None;
    }

    pub fn delete(&mut self, q: &Qso) {
        if let Some(i) = self.find(q) {
            self.log.qsos.remove(i);
            self.scored = None;
        }
    }

    fn rescored(&mut self, mut log: Log) -> (Log, u64) {
        let score = rescore(&mut log, &mut self.contest).map_or(0, |s| s.score);
        (log, score)
    }

    fn scored(&mut self) -> &(Log, u64) {
        if self.scored.is_none() {
            let s = self.rescored(self.log.clone());
            self.scored = Some(s);
        }
        self.scored.as_ref().unwrap()
    }

    pub fn verify(&mut self, q: &Qso) -> Qso {
        let k = key(q);
        let mut log = self.log.clone();
        let at = match self.find(q) {
            Some(i) if log.qsos[i] == k => return self.scored().0.qsos[i].clone(),
            Some(i) => {
                log.qsos[i] = k;
                i
            }
            None => {
                log.qsos.push(k);
                log.qsos.len() - 1
            }
        };
        self.rescored(log).0.qsos.swap_remove(at)
    }

    pub fn total(&mut self) -> u64 {
        self.scored().1
    }

    pub fn cities(&self) -> String {
        self.contest
            .multi_table()
            .into_iter()
            .map(|(code, name, _)| format!("{code} {name}\n"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zlog_core::{Band, Mode};

    const HSTEST: &str = "#high school\nmode on\ncounthigh on\nusemulti2 on\nundefmulti on\nexit ;\nscript lua\n\
function init()\n\treturn {hs = {}}\nend\n\
function add(state, qso)\n\tlocal area, kind = string.match(qso.rcvd, \"^(%d*)(.*)$\")\n\
\tlocal hs = \"\"\n\tif kind == \"HS\" and not state.hs[qso.call] then\n\t\tstate.hs[qso.call] = true\n\t\ths = \"HS\"\n\tend\n\
\treturn state, {score = if qso.mode == \"CW\" then 3 else 1, mul1 = area, mul2 = hs}\nend\n\
function total(state, sums)\n\treturn sums.score * sums.mults\nend\n";

    fn qso(call: &str, rcvd: &str, id: i32) -> Qso {
        Qso {
            callsign: call.into(),
            nr_rcvd: rcvd.into(),
            band: Band::B7,
            mode: Mode::Cw,
            reserve3: id,
            ..Qso::default()
        }
    }

    fn rule() -> Rule {
        let dir = std::env::temp_dir().join(format!("zlog-dll-rule-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("hstest.cfg");
        std::fs::write(&cfg, HSTEST).unwrap();
        Rule::open(&cfg).unwrap()
    }

    #[test]
    fn a_lua_cfg_scores_the_copy_of_the_log() {
        let mut r = rule();
        r.insert(&qso("JA1AAA", "10HS", 1));
        r.insert(&qso("JA1BBB", "10", 2));
        let v = r.verify(&qso("JA1AAA", "10HS", 1));
        assert_eq!(
            (v.points, v.multi1.as_str(), v.multi2.as_str()),
            (3, "10", "HS")
        );
        let fresh = r.verify(&qso("JA1CCC", "11HS", 0));
        assert_eq!((fresh.points, fresh.multi1.as_str()), (3, "11"));
        assert!(fresh.new_multi1);
        assert_eq!(r.total(), 6 * 2);
        r.delete(&qso("JA1BBB", "", 2));
        assert_eq!(r.total(), 3 * 2);
    }

    #[test]
    fn an_edited_qso_is_scored_as_edited() {
        let mut r = rule();
        r.insert(&qso("JA1AAA", "10", 1));
        let v = r.verify(&qso("JA1AAA", "12HS", 1));
        assert_eq!((v.multi1.as_str(), v.multi2.as_str()), ("12", "HS"));
        assert_eq!(r.verify(&qso("JA1AAA", "10", 1)).multi2, "");
    }
}
