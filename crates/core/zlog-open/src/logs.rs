use std::path::{Path, PathBuf};

use zlog_ui::{Section, SettingsPage};

use crate::tr;

pub const LOGS: &str = "logs";

const AUTO: &str = "auto";

const FOLDER: &str = "folder";

const PATTERN: &str = "pattern";

pub const DEFAULT_PATTERN: &str = "{year}/{contest}/{call}.zlox";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogRule {
    pub auto: bool,
    pub folder: String,
    pub pattern: String,
}

pub fn default_folder() -> PathBuf {
    dirs::document_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join(zlog_ui::APP_DIR)
        .join("logs")
}

impl LogRule {
    pub fn load(s: &Section) -> Self {
        LogRule {
            auto: s.get(AUTO).is_some_and(|v| v == "1"),
            folder: s.get(FOLDER).cloned().unwrap_or_default(),
            pattern: s.get(PATTERN).cloned().unwrap_or_default(),
        }
    }

    pub fn path(&self, contest: &str, cfg: &str, call: &str, date: (i32, u32, u32)) -> PathBuf {
        let folder = self.root();
        let pattern = match self.pattern.trim() {
            "" => DEFAULT_PATTERN,
            p => p,
        };
        let (y, m, d) = date;
        let year = y.to_string();
        let day = format!("{y:04}{m:02}{d:02}");
        let call = match call.trim() {
            "" => "log",
            c => c,
        };
        folder.join(zlog_core::file::expand_pattern(
            pattern,
            &[
                ("year", &year),
                ("date", &day),
                ("contest", contest),
                ("cfg", cfg),
                ("call", call),
            ],
        ))
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct LogTree {
    pub name: String,
    pub dirs: Vec<LogTree>,
    pub files: Vec<PathBuf>,
}

const LOG_EXTENSIONS: &[&str] = &["zlox", "zlo"];

const MAX_DEPTH: usize = 4;

impl LogTree {
    pub fn scan(folder: &Path) -> Self {
        Self::walk(folder, MAX_DEPTH)
    }

    fn walk(dir: &Path, depth: usize) -> Self {
        let mut tree = LogTree {
            name: dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            ..LogTree::default()
        };
        let entries = std::fs::read_dir(dir).into_iter().flatten().flatten();
        for p in entries.map(|e| e.path()) {
            if p.is_dir() && depth > 0 {
                let sub = Self::walk(&p, depth - 1);
                if !sub.is_empty() {
                    tree.dirs.push(sub);
                }
            } else if p.is_file() && is_log(&p) {
                tree.files.push(p);
            }
        }
        tree.dirs
            .sort_by(|a, b| match (year(&a.name), year(&b.name)) {
                (Some(x), Some(y)) => y.cmp(&x),
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            });
        tree.files.sort();
        tree
    }

    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty() && self.files.is_empty()
    }
}

fn year(name: &str) -> Option<u32> {
    name.parse().ok()
}

fn is_log(p: &Path) -> bool {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| LOG_EXTENSIONS.contains(&e.as_str()))
}

impl LogRule {
    pub fn root(&self) -> PathBuf {
        match self.folder.trim() {
            "" => default_folder(),
            f => PathBuf::from(f),
        }
    }
}

pub fn today() -> (i32, u32, u32) {
    use chrono::Datelike;
    let d = chrono::Local::now().date_naive();
    (d.year(), d.month(), d.day())
}

pub fn cfg_name(file: Option<&Path>, id: &str) -> String {
    file.and_then(|f| f.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| id.to_string())
}

#[derive(Default)]
pub struct LogOptions;

impl SettingsPage for LogOptions {
    fn id(&self) -> &str {
        LOGS
    }

    fn title(&self, language: &str) -> String {
        tr(language, "logs.title").into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, section: &mut Section, lang: &str) {
        let before = LogRule::load(section);
        let mut rule = before.clone();
        ui.checkbox(&mut rule.auto, tr(lang, "logs.auto"))
            .on_hover_text(tr(lang, "logs.auto.tip"));
        ui.add_enabled_ui(rule.auto, |ui| {
            let mut browse = false;
            let labels = [tr(lang, "logs.folder"), tr(lang, "logs.pattern")];
            zlog_ui::form(ui, "logs_rule", &labels, |ui, form| {
                ui.label(tr(lang, "logs.folder"));
                let hint = default_folder().display().to_string();
                form.text(
                    ui,
                    egui::TextEdit::singleline(&mut rule.folder).hint_text(hint),
                );
                ui.end_row();
                ui.label("");
                browse = ui.button(tr(lang, "logs.browse")).clicked();
                ui.end_row();
                ui.label(tr(lang, "logs.pattern"));
                form.text(
                    ui,
                    egui::TextEdit::singleline(&mut rule.pattern).hint_text(DEFAULT_PATTERN),
                )
                .on_hover_text(tr(lang, "logs.pattern.tip"));
                ui.end_row();
            });
            if browse {
                if let Some(p) = zlog_ui::dialog::pick_folder(&rule.folder) {
                    rule.folder = p.display().to_string();
                }
            }
            let example = rule.path("ALL JA", "alljajp", "JA1ZLO", today());
            ui.weak(example.display().to_string());
        });
        if rule != before {
            section.insert(AUTO.into(), if rule.auto { "1" } else { "0" }.into());
            section.insert(FOLDER.into(), rule.folder);
            section.insert(PATTERN.into(), rule.pattern);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_names_logs_by_year_contest_and_call() {
        let rule = LogRule {
            auto: true,
            folder: "/logs".into(),
            pattern: String::new(),
        };
        let p = rule.path("ALL JA1", "allja1", "JA1ZLO/1", (2026, 6, 7));
        assert_eq!(
            p,
            Path::new("/logs")
                .join("2026")
                .join("ALL JA1")
                .join("JA1ZLO-1.zlox")
        );
        let rule = LogRule {
            pattern: "{date}-{cfg}.zlox".into(),
            ..rule
        };
        assert_eq!(
            rule.path("x", "allja1", "", (2026, 6, 7)),
            Path::new("/logs").join("20260607-allja1.zlox")
        );
    }

    #[test]
    fn the_tree_groups_logs_by_folder_newest_year_first() {
        let root = std::env::temp_dir().join(format!("zlog-logtree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for p in [
            "2025/ALL JA1/JA1ZLO.zlox",
            "2026/ALL JA1/JA1ZLO.zlox",
            "2026/6m AND DOWN/JA1ZLO.ZLO",
            "2026/6m AND DOWN/notes.txt",
            "2024/empty/readme.txt",
        ] {
            let p = root.join(p);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
        let tree = LogTree::scan(&root);
        let names = |t: &LogTree| t.dirs.iter().map(|d| d.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&tree), ["2026", "2025"]);
        assert_eq!(names(&tree.dirs[0]), ["6m AND DOWN", "ALL JA1"]);
        assert_eq!(tree.dirs[0].dirs[0].files.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_rule_reads_its_section() {
        let mut s = Section::new();
        assert!(!LogRule::load(&s).auto);
        s.insert(AUTO.into(), "1".into());
        s.insert(PATTERN.into(), "{call}.zlox".into());
        let r = LogRule::load(&s);
        assert!(r.auto);
        assert_eq!(r.pattern, "{call}.zlox");
    }
}
