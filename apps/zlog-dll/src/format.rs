use std::path::Path;

use zlog_core::{Contest, Extension, Format, Log, LogFormat};
use zlog_qxsl::Qxsl;

fn formats() -> Vec<Box<dyn LogFormat>> {
    Qxsl.formats()
}

fn pattern(f: &dyn LogFormat) -> String {
    let p: Vec<String> = f.extensions().iter().map(|e| format!("*.{e}")).collect();
    p.join(";")
}

fn label(f: &dyn LogFormat) -> String {
    format!("{} ({})", f.name(), pattern(f))
}

pub fn filter() -> String {
    let all: Vec<String> = formats()
        .iter()
        .filter(|f| f.can_import() && f.can_export() && !f.extensions().is_empty())
        .map(|f| format!("{}|{}", label(f.as_ref()), pattern(f.as_ref())))
        .collect();
    all.join("|")
}

pub fn import(source: &Path, target: &Path, format: Format) -> Result<(), String> {
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = std::fs::read(source).map_err(|e| e.to_string())?;
    let qsos = formats()
        .iter()
        .filter(|f| f.can_import() && f.extensions().iter().any(|e| e.eq_ignore_ascii_case(&ext)))
        .filter_map(|f| f.import(&bytes).ok())
        .find(|q| !q.is_empty())
        .ok_or_else(|| format!("{}: no format reads it", source.display()))?;
    let log = Log {
        qsos,
        ..Log::default()
    };
    std::fs::write(target, log.to_bytes(format)).map_err(|e| e.to_string())
}

pub fn export(target: &Path, name: &str, contest: &dyn Contest) -> Result<(), String> {
    let (log, _) = Log::load(target).map_err(|e| e.to_string())?;
    let all = formats();
    let f = all
        .iter()
        .find(|f| label(f.as_ref()) == name || f.name().eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("{name}: unknown format"))?;
    let bytes = f.export(&log, contest)?;
    std::fs::write(target, bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zlog_cfg::UserDefinedDef;
    use zlog_core::{Band, ContestMode, Mode, Qso};

    #[test]
    fn the_filter_pairs_labels_and_patterns() {
        let f = filter();
        let parts: Vec<&str> = f.split('|').collect();
        assert_eq!(parts.len() % 2, 0);
        assert!(parts.iter().any(|p| p.starts_with("ADIF")));
        assert!(parts.chunks(2).all(|c| c[1].starts_with("*.")));
    }

    #[test]
    fn a_log_goes_out_as_adif_and_comes_back() {
        let dir = std::env::temp_dir().join(format!("zlog-dll-format-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zlo = dir.join("log.zlo");
        let log = Log {
            qsos: vec![Qso {
                callsign: "JA1ZLO".into(),
                nr_rcvd: "100".into(),
                band: Band::B7,
                mode: Mode::Cw,
                time: zlog_core::DelphiTime(46000.5),
                ..Qso::default()
            }],
            ..Log::default()
        };
        std::fs::write(&zlo, log.to_bytes(Format::Zlo)).unwrap();
        let contest = UserDefinedDef::from_cfg("", |_| None).build_rules(ContestMode::All);
        let adif = formats()
            .into_iter()
            .find(|f| f.name().starts_with("ADIF"))
            .unwrap();
        export(&zlo, &label(adif.as_ref()), &contest).unwrap();
        let text = std::fs::read_to_string(&zlo).unwrap();
        assert!(text.contains("JA1ZLO"));
        let source = dir.join(format!("log.{}", adif.extensions()[0]));
        std::fs::rename(&zlo, &source).unwrap();
        let back = dir.join("back.zlo");
        import(&source, &back, Format::Zlox).unwrap();
        let (read, format) = Log::load(&back).unwrap();
        assert_eq!(format, Format::Zlox);
        assert_eq!(read.qsos.len(), 1);
        assert_eq!(read.qsos[0].callsign, "JA1ZLO");
        assert_eq!(read.qsos[0].band, Band::B7);
    }
}
