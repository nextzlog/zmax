use egui_kittest::kittest::NodeT;
use zlog_gui::testing::i18n::tr;
use zlog_testing::{click, harness, open_log, setup, Env};

#[test]
fn a_docked_cw4isr_fills_the_entry_by_itself_and_by_a_pointer_click() {
    fill_from("CQ TEST JH1ABC JH1ABC TEST");
}

#[test]
fn a_callsign_sent_once_still_fills_the_entry() {
    fill_from("JH1ABC");
}

fn fill_from(text: &str) {
    zlog_gui::testing::registry::install(zlog_max::register);
    let env = Env::new(&format!("cw4isr-{}", text.len()));
    std::fs::write(
        env.dir.join("settings.toml"),
        "[preferences]\nentry_style = \"zlog\"\n[dock]\nright = \"cw-decoder\"\n[\"panel.cw-decoder\"]\n\"extension.auto\" = \"true\"\n",
    )
    .unwrap();
    let mut h = harness();
    open_log(&mut h, setup(&env.dir, "allja", None));
    let panel = zlog_gui::testing::registry::panels()
        .into_iter()
        .find(|p| p.id() == "cw-decoder")
        .unwrap();
    let window = zlog_ui::panel_commands(panel.as_ref(), false, "en")
        .into_iter()
        .find(|c| c.is_window())
        .unwrap();
    click(
        &mut h,
        tr(zlog_gui::testing::menus::group_key(panel.menu())),
    );
    click(&mut h, &window.menu_text());
    h.run_steps(5);
    let path = env.dir.join("cq.wav");
    let rate = zlog_cw4isr::sim::RATE;
    let audio = zlog_cw4isr::sim::signal(text, rate);
    zlog_cw4isr::sim::write_wav(&path, &audio, rate).unwrap();
    zlog_testing::pick(Some(&path));
    click(&mut h, "Open…");
    h.run_steps(3);
    zlog_testing::pick(None);
    let call = |h: &zlog_testing::H| h.state().entry().unwrap().callsign;
    let start = std::time::Instant::now();
    while call(&h).is_empty() {
        assert!(start.elapsed() < std::time::Duration::from_secs(30));
        std::thread::sleep(std::time::Duration::from_millis(50));
        h.run_steps(1);
    }
    assert_eq!(call(&h), "JH1ABC");
    let mut entry = h.state().entry().unwrap();
    entry.callsign.clear();
    h.state_mut().set_entry(entry);
    h.run_steps(2);
    let chip = h
        .root()
        .children_recursive()
        .find(|n| {
            let a = n.accesskit_node();
            a.role() == egui::accesskit::Role::Button && a.label().unwrap_or_default() == "JH1ABC"
        })
        .unwrap()
        .rect()
        .center();
    zlog_testing::click_at(&mut h, chip, egui::PointerButton::Primary, 1);
    assert_eq!(call(&h), "JH1ABC");
}
