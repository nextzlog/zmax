mod format;
mod record;

mod rule;

use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::Mutex;

use encoding_rs::SHIFT_JIS;
use zlog_cfg::UserDefinedDef;
use zlog_core::{ContestMode, Format};

use crate::rule::Rule;

type Str = *const c_char;
type Give = extern "C" fn(Str);

struct State {
    format: Format,
    rule: Option<Rule>,
}

static STATE: Mutex<State> = Mutex::new(State {
    format: Format::Zlo,
    rule: None,
});

fn with<T>(fallback: T, f: impl FnOnce(&mut State) -> T) -> T {
    catch_unwind(AssertUnwindSafe(|| {
        f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
    }))
    .unwrap_or(fallback)
}

fn text(p: Str) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: zLog passes NUL-terminated strings.
    let bytes = unsafe { CStr::from_ptr(p) }.to_bytes();
    SHIFT_JIS.decode(bytes).0.into_owned()
}

fn give(f: Give, s: &str) {
    let c = CString::new(s.replace('\0', "")).unwrap_or_default();
    f(c.as_ptr());
}

fn record<'a>(p: usize, format: Format) -> Option<&'a mut [u8]> {
    // SAFETY: zLog passes a whole record of the size agreed by `zylo_record_size`.
    (p != 0).then(|| unsafe { std::slice::from_raw_parts_mut(p as *mut u8, format.record_size()) })
}

#[no_mangle]
pub extern "C" fn zylo_record_size(size: i32) -> i32 {
    with(0, |s| {
        s.format = record::format_of(size.max(0) as usize);
        s.format.record_size() as i32
    })
}

#[no_mangle]
pub extern "C" fn zylo_query_format(f: Give) {
    with((), |_| give(f, &format::filter()))
}

#[no_mangle]
pub extern "C" fn zylo_query_cities(f: Give) {
    with((), |s| {
        give(f, &s.rule.as_ref().map(Rule::cities).unwrap_or_default())
    })
}

#[no_mangle]
pub extern "C" fn zylo_launch_event() -> bool {
    true
}

#[no_mangle]
pub extern "C" fn zylo_finish_event() -> bool {
    with(false, |s| {
        s.rule = None;
        false
    })
}

#[no_mangle]
pub extern "C" fn zylo_window_event(_: usize) {}

#[no_mangle]
pub extern "C" fn zylo_import_event(source: Str, target: Str) -> bool {
    let (source, target) = (text(source), text(target));
    with(false, |s| {
        format::import(Path::new(&source), Path::new(&target), s.format).is_ok()
    })
}

#[no_mangle]
pub extern "C" fn zylo_export_event(target: Str, name: Str) -> bool {
    let (target, name) = (text(target), text(name));
    with(false, |s| {
        let plain;
        let contest = match &s.rule {
            Some(r) => r.contest(),
            None => {
                plain = UserDefinedDef::from_cfg("", |_| None).build_rules(ContestMode::All);
                &plain
            }
        };
        format::export(Path::new(&target), &name, contest).is_ok()
    })
}

#[no_mangle]
pub extern "C" fn zylo_attach_event(_: Str, _: Str) {}

#[no_mangle]
pub extern "C" fn zylo_assign_event(_: Str, cfg: Str) {
    let cfg = text(cfg);
    with((), |s| s.rule = Rule::open(Path::new(&cfg)).ok())
}

#[no_mangle]
pub extern "C" fn zylo_detach_event(_: Str, _: Str) {
    with((), |s| s.rule = None)
}

#[no_mangle]
pub extern "C" fn zylo_offset_event(_: i32) {}

#[no_mangle]
pub extern "C" fn zylo_insert_event(p: usize) {
    with((), |s| {
        if let (Some(r), Some(bytes)) = (s.rule.as_mut(), record(p, s.format)) {
            r.insert(&record::decode(bytes, s.format));
        }
    })
}

#[no_mangle]
pub extern "C" fn zylo_delete_event(p: usize) {
    with((), |s| {
        if let (Some(r), Some(bytes)) = (s.rule.as_mut(), record(p, s.format)) {
            r.delete(&record::decode(bytes, s.format));
        }
    })
}

#[no_mangle]
pub extern "C" fn zylo_verify_event(p: usize) {
    with((), |s| {
        if let (Some(r), Some(bytes)) = (s.rule.as_mut(), record(p, s.format)) {
            let scored = r.verify(&record::decode(bytes, s.format));
            record::patch(bytes, &scored, s.format);
        }
    })
}

#[no_mangle]
pub extern "C" fn zylo_points_event(points: i32, multis: i32) -> i32 {
    with(points.saturating_mul(multis), |s| match s.rule.as_mut() {
        Some(r) => i32::try_from(r.total()).unwrap_or(i32::MAX),
        None => points.saturating_mul(multis),
    })
}

#[no_mangle]
pub extern "C" fn zylo_button_event(_: i32, _: i32) {}

#[no_mangle]
pub extern "C" fn zylo_editor_event(_: i32, _: i32) {}
