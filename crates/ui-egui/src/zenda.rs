//! Zenda patches for Work. Events carry metadata only; Work owns accounts and Theo.
//!
//! While Work drives this copy over the control channel it drains the events with `work.events`.
//! The copy Work opens for the person to keep editing has no control channel (DeckCraft's channel
//! has no key, so Work never leaves one open): there the Theo and Share buttons run the `zwork.exe`
//! Work named in `ZWORK_EXE` (an absolute path to a file named `zwork.exe`, or nothing), one
//! argument per item, never through a shell:
//!
//! - Theo: `zwork.exe theo presentaciones <pid> <from>`
//! - Share: `zwork.exe compartir presentaciones <pid> <dirty 0|1> <name> <path>`
//!
//! `<pid>` is this process, so Work can put itself beside this window; `<from>` is a short ASCII
//! word (`button`); `<name>` (at most 200 characters) and `<path>` are `""` when there is none
//! (a document never saved has no path). The child is waited for off the UI thread: if it exits
//! with an error (a Work older than these commands answers «No conozco el comando…»), or cannot
//! start, `zwork.exe abrir guia` runs instead, so that Work still comes forward.
use serde_json::{Value, json};

pub const ON: bool = cfg!(feature = "zenda");

/// Events kept for Work when it does not drain them; older ones are dropped first.
const OUTBOX_MAX: usize = 32;

/// What the person asked of Work, oldest first, until Work drains it (`work.events`).
#[derive(Default, Debug)]
pub struct Outbox {
    events: Vec<Value>,
    seq: u64,
    /// Started with `--control`: Work drains this outbox. (`control_rx` is taken while a request
    /// runs, so it cannot tell.)
    pub channel: bool,
}

impl Outbox {
    /// Queue `event` (`work.theo`, `work.share`) with its params.
    pub fn push(&mut self, event: &str, params: Value) {
        self.seq = self.seq.saturating_add(1);
        if self.events.len() == OUTBOX_MAX {
            self.events.remove(0);
        }
        self.events.push(json!({"seq": self.seq, "event": event, "params": params}));
    }

    /// Every queued event, oldest first; the outbox is empty afterwards.
    pub fn drain(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.events)
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// Where help was requested, without presentation content.
pub fn theo_context(app: &crate::SlideApp) -> Value {
    json!({
        "from": "button",
        "tab": app.ui.tab,
        "hasDocument": app.session.active().is_some(),
        "slide": app.session.active().map(|d| d.selection.slide),
    })
}

/// The saved presentation, including whether its disk copy is stale.
pub fn share_params(app: &crate::SlideApp) -> Value {
    let doc = app.session.active();
    json!({"path": doc.and_then(|d| d.path.as_deref()), "dirty": doc.is_some_and(|d| d.is_dirty()), "name": doc.map(|d| d.title())})
}

/// Queue `event` for Work, or, without a control channel, ask the Work that opened this copy to
/// come forward. Returns what the caller reports (`{"work": "theo"}`).
pub fn reach_work(app: &mut crate::SlideApp, event: &str, params: Value) -> Value {
    let kind = event.trim_start_matches("work.");
    if app.zenda.channel || app.control_rx.is_some() || !cfg!(not(target_arch = "wasm32")) {
        app.zenda.push(event, params);
        return json!({"work": kind, "via": "channel"});
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(exe) = work_exe(std::env::var_os("ZWORK_EXE")) {
        let started = ask_work(exe, zwork_args(kind, std::process::id(), &params));
        return json!({"work": kind, "via": "zwork", "ok": started});
    }
    app.zenda.push(event, params);
    json!({"work": kind, "via": "none"})
}

/// The tool id Work knows this app by.
#[cfg(not(target_arch = "wasm32"))]
const TOOL: &str = "presentaciones";
/// The longest presentation name passed to Work, in characters.
#[cfg(not(target_arch = "wasm32"))]
const NAME_MAX: usize = 200;
/// What an older Work understands: it only comes forward.
#[cfg(not(target_arch = "wasm32"))]
const FALLBACK: [&str; 2] = ["abrir", "guia"];

/// `zwork.exe` arguments for `kind` (`theo`, `share`) from this process `pid`, or `None` for an
/// event Work has no command for.
#[cfg(not(target_arch = "wasm32"))]
fn zwork_args(kind: &str, pid: u32, params: &Value) -> Option<Vec<String>> {
    let text = |key: &str| params.get(key).and_then(Value::as_str).unwrap_or_default();
    match kind {
        "theo" => {
            let from = text("from");
            let ok = !from.is_empty() && from.len() <= 32 && from.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
            Some(vec!["theo".into(), TOOL.into(), pid.to_string(), if ok { from } else { "button" }.into()])
        }
        "share" => {
            let dirty = params.get("dirty").and_then(Value::as_bool).unwrap_or(false);
            let name: String = text("name").chars().filter(|c| !c.is_control()).take(NAME_MAX).collect();
            Some(vec!["compartir".into(), TOOL.into(), pid.to_string(), if dirty { "1" } else { "0" }.into(), name, text("path").into()])
        }
        _ => None,
    }
}

/// Whether `zwork.exe abrir guia` must run after the new command: Work answered it with an error,
/// or the child could not be waited for (`None`) or ended without an exit code.
#[cfg(not(target_arch = "wasm32"))]
fn needs_fallback(code: Option<Option<i32>>) -> bool {
    !matches!(code, Some(Some(0)))
}

/// Runs `exe args` and, off the UI thread, falls back to `exe abrir guia` when it fails; without
/// `args`, or when it cannot start, the fallback runs right away. Returns whether something started.
#[cfg(not(target_arch = "wasm32"))]
fn ask_work(exe: std::path::PathBuf, args: Option<Vec<String>>) -> bool {
    let fallback = |exe: &std::path::Path| std::process::Command::new(exe).args(FALLBACK).spawn().is_ok();
    match args.map(|a| std::process::Command::new(&exe).args(a).spawn()) {
        Some(Ok(mut child)) => {
            std::thread::spawn(move || {
                if needs_fallback(child.wait().ok().map(|s| s.code())) {
                    fallback(&exe);
                }
            });
            true
        }
        _ => fallback(&exe),
    }
}

/// The `zwork.exe` that started this copy: an absolute path to a file named `zwork.exe`.
#[cfg(not(target_arch = "wasm32"))]
fn work_exe(var: Option<std::ffi::OsString>) -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(var?);
    let named = path.file_name().is_some_and(|n| n.eq_ignore_ascii_case("zwork.exe"));
    (path.is_absolute() && named && path.is_file()).then_some(path)
}

/// Work's mascot and orange brand button, shared by the curated app patches.
#[cfg(feature = "zenda")]
pub fn theo_button(ui: &mut egui::Ui, label: &str, hint: &str) -> egui::Response {
    let key = egui::Id::unique("zenda.theo.texture");
    let texture = ui.ctx().data_mut(|d| d.get_temp::<egui::TextureHandle>(key)).or_else(|| {
        let image = image::load_from_memory(include_bytes!("../../../assets/zenda/theo-icon-64.png")).ok()?.to_rgba8();
        let color = egui::ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], image.as_raw());
        let texture = ui.ctx().load_texture("Theo mascot", color, egui::TextureOptions::LINEAR);
        ui.ctx().data_mut(|d| d.insert_temp(key, texture.clone()));
        Some(texture)
    });
    let text = egui::RichText::new("Theo").color(egui::Color32::BLACK).size(11.5);
    let button = if let Some(texture) = texture {
        egui::Button::image_and_text(egui::Image::new((texture.id(), egui::vec2(18.0, 18.0))), text)
    } else {
        egui::Button::new(text)
    };
    let response = ui.add(button.fill(egui::Color32::from_rgb(234, 108, 30)).corner_radius(11.0).min_size(egui::vec2(74.0, 24.0)));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response.on_hover_text(hint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    fn app() -> crate::SlideApp {
        crate::SlideApp::new(deckcraft_engine::Session::with_new(), crate::Services::default())
    }

    #[test]
    fn events_drain_in_order_and_are_bounded() {
        let mut out = Outbox::default();
        for _ in 0..40 {
            out.push("work.theo", json!({"from": "button"}));
        }
        let got = out.drain();
        assert_eq!(got.len(), 32);
        assert_eq!(got[0]["seq"], 9);
        assert!(out.is_empty());
    }

    #[test]
    fn only_zenda_exposes_work_events() {
        let mut app = app();
        app.zenda.push("work.theo", theo_context(&app));
        let (reply, _rx) = std::sync::mpsc::channel();
        let req = crate::control::ControlRequest { method: "work.events".into(), params: json!({}), reply, state: Default::default() };
        let crate::control::Outcome::Done(reply) = crate::control::handle(&mut app, &egui::Context::default(), &req) else {
            panic!("immediate response")
        };
        assert_eq!(reply["ok"], ON);
        if ON {
            assert_eq!(reply["result"]["events"][0]["event"], "work.theo");
            assert!(reply["result"]["events"][0]["params"].get("text").is_none());
        }
    }

    #[test]
    fn share_preserves_unsaved_state_and_only_queues_in_zenda() {
        let mut app = app();
        app.run("slide.new", json!({"layout": "blank"})).unwrap();
        let result = app.run("file.share", json!({}));
        if ON {
            assert_eq!(result.unwrap()["work"], "share");
            let got = app.zenda.drain();
            assert_eq!(got[0]["event"], "work.share");
            assert_eq!(got[0]["params"]["path"], Value::Null);
            assert_eq!(got[0]["params"]["dirty"], true);
        } else {
            assert!(result.is_err());
            assert!(app.zenda.is_empty());
        }
    }

    #[test]
    fn work_exe_must_be_an_absolute_zwork_exe() {
        let this = std::env::current_exe().unwrap();
        for bad in ["zwork.exe".into(), this.into_os_string(), std::env::temp_dir().join("no-such-dir").join("zwork.exe").into_os_string()] {
            assert!(work_exe(Some(bad.clone())).is_none(), "{bad:?}");
        }
        assert!(work_exe(None).is_none());
    }

    #[test]
    fn theo_args_carry_tool_pid_and_a_safe_from() {
        let args = zwork_args("theo", 4321, &json!({"from": "button", "tab": "Home"})).unwrap();
        assert_eq!(args, ["theo", "presentaciones", "4321", "button"]);
        for bad in [json!({}), json!({"from": "a b"}), json!({"from": "x".repeat(40)}), json!({"from": 3})] {
            assert_eq!(zwork_args("theo", 1, &bad).unwrap()[3], "button", "{bad}");
        }
    }

    #[test]
    fn share_args_carry_dirty_name_and_path_with_empty_for_none() {
        let saved = json!({"path": r"C:\Users\Ana\Clase.pptx", "dirty": false, "name": "Clase"});
        assert_eq!(zwork_args("share", 7, &saved).unwrap(), ["compartir", "presentaciones", "7", "0", "Clase", r"C:\Users\Ana\Clase.pptx"]);
        let unsaved = json!({"path": null, "dirty": true, "name": null});
        assert_eq!(zwork_args("share", 7, &unsaved).unwrap(), ["compartir", "presentaciones", "7", "1", "", ""]);
        let long = json!({"name": format!("{}\n", "ñ".repeat(300))});
        let name = &zwork_args("share", 7, &long).unwrap()[4];
        assert_eq!(name.chars().count(), NAME_MAX);
        assert!(!name.contains('\n'));
        assert!(zwork_args("other", 7, &json!({})).is_none());
    }

    #[test]
    fn only_a_clean_exit_skips_the_fallback() {
        assert!(!needs_fallback(Some(Some(0))));
        for failed in [Some(Some(1)), Some(None), None] {
            assert!(needs_fallback(failed), "{failed:?}");
        }
    }

    #[cfg(feature = "zenda")]
    #[test]
    fn theo_button_has_mascot_orange_and_accessible_names() {
        use egui_kittest::kittest::Queryable;
        for (language, label) in [(Language::En, "Ask Theo for help"), (Language::Es, "Pedile ayuda a Theo"), (Language::PtBr, "Peça ajuda ao Theo")]
        {
            let mut h = egui_kittest::Harness::builder().build_ui_state(
                |ui, count: &mut usize| {
                    if theo_button(ui, language.tr("Ask Theo for help"), "Theo").clicked() {
                        *count += 1;
                    }
                },
                0usize,
            );
            h.get_by_label(label).click();
            h.run_steps(2);
            assert_eq!(*h.state(), 1);
        }
        let image = image::load_from_memory(include_bytes!("../../../assets/zenda/theo-icon-64.png")).unwrap();
        assert_eq!((image.width(), image.height()), (64, 64));
    }

    /// The real title bar and ribbon, in the three languages: Theo and Share reach Work, and the
    /// controls the first lessons name are translated. `ZENDA_TREE_DIR` dumps each tree.
    #[test]
    fn title_bar_theo_share_and_ribbon_in_three_languages() {
        use egui_kittest::kittest::Queryable;
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut crate::SlideApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app(),
        );
        h.run_steps(8);
        if ON {
            h.get_by_label("Ask Theo for help").click();
            h.run_steps(2);
            h.get_by_label("Share").click();
            h.run_steps(2);
            let events = h.state_mut().zenda.drain();
            assert_eq!(events.len(), 2, "{events:?}");
            assert_eq!(events[0]["event"], "work.theo");
            assert_eq!(events[1]["event"], "work.share");
        } else {
            assert!(h.query_by_label("Ask Theo for help").is_none());
        }
        for (language, home, new_slide, layout, insert) in [
            (Language::Es, "Inicio", "Nueva diapositiva", "Disposición", "Insertar"),
            (Language::PtBr, "Página Inicial", "Novo Slide", "Layout", "Inserir"),
            (Language::En, "Home", "New Slide", "Layout", "Insert"),
        ] {
            crate::i18n::set(language);
            h.state_mut().ui.tab = "Home".into();
            h.run_steps(4);
            for name in [home, new_slide, layout, insert] {
                assert!(h.query_all_by_label(name).next().is_some(), "{}: {name}", language.code());
            }
            if language == Language::Es && !cfg!(target_os = "macos") {
                // Zenda's E2E of Work 0.2.30: the search button read «(⇧⌘P)», a Mac shortcut, on Windows.
                assert!(h.query_all_by_label("Buscar comandos (Ctrl+Shift+P)").next().is_some(), "es: Windows keys");
            }
            if ON {
                assert!(h.query_all_by_label(language.tr("Ask Theo for help")).next().is_some(), "{}: Theo", language.code());
                assert!(h.query_all_by_label(language.tr("Share")).next().is_some(), "{}: Share", language.code());
            }
            if let Some(dir) = std::env::var_os("ZENDA_TREE_DIR") {
                for tab in ["Home", "Design", "Transitions"] {
                    h.state_mut().ui.tab = tab.into();
                    h.run_steps(4);
                    let path = std::path::PathBuf::from(&dir).join(format!("deckcraft-{}-{tab}.txt", language.code()));
                    std::fs::write(path, format!("{:#?}", h.kittest_state().root())).unwrap();
                }
            }
        }
        crate::i18n::set(Language::En);
    }
}
