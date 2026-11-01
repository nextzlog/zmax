use std::ffi::{c_char, CString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use zlog_core::zlo::{ansi, decode_record, encode_record};
use zlog_core::{Band, DelphiTime, Format, Log, Mode, Qso};
use zlog_dll::*;

static SERIAL: Mutex<()> = Mutex::new(());

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zlog-dll-abi-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn c(s: &str) -> CString {
    CString::new(ansi(s)).unwrap()
}

fn open(size: i32, cfg: &Path) -> Format {
    let format = match zylo_record_size(size) {
        384 => Format::Zlox,
        _ => Format::Zlo,
    };
    let (name, path) = (c("TEST"), c(&cfg.display().to_string()));
    zylo_attach_event(name.as_ptr(), path.as_ptr());
    zylo_assign_event(name.as_ptr(), path.as_ptr());
    format
}

fn qso(call: &str, rcvd: &str, band: Band, mode: Mode, id: i32) -> Qso {
    Qso {
        time: DelphiTime(46000.5 + id as f64 / 1440.0),
        callsign: call.into(),
        nr_rcvd: rcvd.into(),
        band,
        mode,
        reserve3: id,
        ..Qso::default()
    }
}

fn insert(q: &Qso, format: Format) {
    let mut r = encode_record(q, format);
    zylo_insert_event(r.as_mut_ptr() as usize);
}

fn delete(q: &Qso, format: Format) {
    let mut r = encode_record(q, format);
    zylo_delete_event(r.as_mut_ptr() as usize);
}

fn verify(q: &Qso, format: Format) -> Qso {
    let mut r = encode_record(q, format);
    zylo_verify_event(r.as_mut_ptr() as usize);
    assert_eq!(r.len(), format.record_size());
    decode_record(&r, format)
}

fn score(q: &Qso) -> (u8, &str, &str) {
    (q.points, q.multi1.as_str(), q.multi2.as_str())
}

#[test]
fn a_reiwa_without_zlox_scores_in_256_bytes() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let f = open(256, &fixtures().join("hstest.cfg"));
    assert_eq!(f, Format::Zlo);
    let a = qso("JA1AAA", "101HS", Band::B7, Mode::Cw, 1);
    assert_eq!(score(&verify(&a, f)), (3, "101", "HS"));
    insert(&a, f);
    let again = qso("JA1AAA", "101HS", Band::B7, Mode::Cw, 2);
    let v = verify(&again, f);
    assert_eq!(v.multi1, "");
    insert(&again, f);
    insert(&qso("JA1BBB", "102C", Band::B21, Mode::Ssb, 3), f);
    assert_eq!(zylo_points_event(0, 0), 4 * 3);
    delete(&again, f);
    assert_eq!(zylo_points_event(0, 0), 4 * 3);
    zylo_detach_event(c("").as_ptr(), c("").as_ptr());
    assert_eq!(zylo_points_event(5, 2), 10);
}

#[test]
fn the_akita_rule_scores_hokkaido_and_members() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let f = open(384, &fixtures().join("aktest.cfg"));
    assert_eq!(f, Format::Zlox);
    let cases = [
        ("JA8AAA", "0102", 9),
        ("JA8BBB", "0101", 6),
        ("JA1CCC", "1001", 3),
        ("JA1DDD", "1001M", 9),
        ("JA1EEE", "10", 0),
    ];
    for (i, (call, rcvd, points)) in cases.iter().enumerate() {
        let q = qso(call, rcvd, Band::B7, Mode::Cw, i as i32 + 1);
        let v = verify(&q, f);
        assert_eq!(v.points, *points, "{call} {rcvd}");
        assert_eq!(v.multi1.is_empty(), *points == 0, "{call} {rcvd}");
        insert(&q, f);
    }
    assert_eq!(zylo_points_event(0, 0), 27 * 2);
    let mut out = String::new();
    extern "C" fn take(s: *const c_char) {
        let t = unsafe { std::ffi::CStr::from_ptr(s) }.to_bytes();
        *CITIES.lock().unwrap() = String::from_utf8(t.to_vec()).unwrap();
    }
    static CITIES: Mutex<String> = Mutex::new(String::new());
    zylo_query_cities(take);
    out.push_str(&CITIES.lock().unwrap());
    assert!(out.lines().any(|l| l.starts_with("0102 ")));
}

#[test]
fn a_cfg_without_lua_is_scored_by_its_own_lines() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = temp("plain");
    let cfg = dir.join("plain.cfg");
    std::fs::write(&cfg, "#plain\npt7 2222\nundefmulti on\nexit ;\n").unwrap();
    let f = open(384, &cfg);
    let a = qso("JA1AAA", "10", Band::B7, Mode::Cw, 1);
    assert_eq!(score(&verify(&a, f)), (2, "10", ""));
    insert(&a, f);
    insert(&qso("JA1BBB", "10", Band::B7, Mode::Cw, 2), f);
    insert(&qso("JA1CCC", "11", Band::B7, Mode::Cw, 3), f);
    assert_eq!(zylo_points_event(0, 0), 6 * 2);
}

#[test]
fn files_with_japanese_paths_go_both_ways() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    zylo_record_size(384);
    let dir = temp("日本語");
    let zlo = dir.join("記録.zlo");
    let log = Log {
        qsos: vec![qso("JA1ZLO", "100", Band::B7, Mode::Cw, 1)],
        ..Log::default()
    };
    std::fs::write(&zlo, log.to_bytes(Format::Zlox)).unwrap();
    let (path, name) = (c(&zlo.display().to_string()), c("ADIF (ADI) (*.adi)"));
    assert!(zylo_export_event(path.as_ptr(), name.as_ptr()));
    assert!(std::fs::read_to_string(&zlo).unwrap().contains("JA1ZLO"));
    let adi = dir.join("記録.adi");
    std::fs::rename(&zlo, &adi).unwrap();
    let (source, target) = (c(&adi.display().to_string()), c(&zlo.display().to_string()));
    assert!(zylo_import_event(source.as_ptr(), target.as_ptr()));
    let (back, format) = Log::load(&zlo).unwrap();
    assert_eq!(format, Format::Zlox);
    assert_eq!(back.qsos[0].callsign, "JA1ZLO");
    let bad = c("no such format");
    assert!(!zylo_export_event(path.as_ptr(), bad.as_ptr()));
}

#[test]
fn a_large_log_stays_fast() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let f = open(384, &fixtures().join("hstest.cfg"));
    let bands = [Band::B7, Band::B21, Band::B50, Band::B144, Band::B430];
    let n = 3000;
    let start = Instant::now();
    for i in 0..n {
        let q = qso(
            &format!("JA{}A{:03}", i % 10, i / 10 % 1000),
            &format!("{}HS", 101 + i % 40),
            bands[i % bands.len()],
            Mode::Cw,
            i as i32 + 1,
        );
        insert(&q, f);
    }
    assert!(zylo_points_event(0, 0) > 0);
    let loaded = start.elapsed();
    let typing = Instant::now();
    for i in 0..20 {
        let q = qso("JR1NEW", &format!("{}HS", 101 + i), Band::B7, Mode::Cw, 0);
        assert_eq!(verify(&q, f).points, 3);
    }
    let each = typing.elapsed() / 20;
    let old = verify(&qso("JR1OLD", "141HS", Band::B7, Mode::Cw, 10), f);
    assert_eq!(old.points, 3);
    eprintln!("{n} QSOs loaded in {loaded:?}, {each:?} per verify");
    let limit = if cfg!(debug_assertions) { 500 } else { 50 };
    assert!(each < Duration::from_millis(limit), "{each:?} per verify");
}
