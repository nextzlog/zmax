fn main() -> zlog_gui::Result {
    let icon = zlog_gui::IconData {
        rgba: include_bytes!(concat!(env!("OUT_DIR"), "/icon.rgba")).to_vec(),
        width: 256,
        height: 256,
    };
    zlog_gui::run(zlog_max::register, icon)
}
