use egui::os::OperatingSystem;
use egui::{Key, Modifiers};
use serde_json::json;
use soundcraft_ui_egui::{Services, SoundApp, shortcuts};

fn press(ctx: &egui::Context, app: &mut SoundApp, key: Key, modifiers: Modifiers) {
    let events = [true, false].map(|pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers }).to_vec();
    ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| shortcuts::handle(app, ui.ctx())).textures_delta.clear();
}

#[test]
fn pro_tools_keys_run_their_commands() {
    let shift = Modifiers::SHIFT;
    let alt = Modifiers::ALT;
    let alt_shift = Modifiers { alt: true, shift: true, ..Modifiers::NONE };
    let pc_ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
    let mac_cmd = Modifiers { mac_cmd: true, command: true, ..Modifiers::NONE };
    for (os, cmd) in [(OperatingSystem::Mac, mac_cmd), (OperatingSystem::Windows, pc_ctrl), (OperatingSystem::Nix, pc_ctrl)] {
        let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
        let kick = app.engine.session().track_by_name("Kick").unwrap().clips()[0].clone();
        app.engine.execute("edit.select", &json!({"tracks": ["Kick"], "start": kick.start, "end": kick.end(), "clips": [kick.id.0]})).unwrap();
        app.engine.execute("edit.selection_to_timeline", &json!({})).unwrap();
        app.engine.execute("edit.copy", &json!({})).unwrap();
        let ctx = egui::Context::default();
        ctx.set_os(os);
        for (key, mods, id) in [
            (Key::A, shift, "track.bypass_inserts"),
            (Key::Num2, shift, "track.bypass_inserts_ae"),
            (Key::Num3, shift, "track.bypass_inserts_fj"),
            (Key::E, shift, "track.bypass_eq"),
            (Key::C, shift, "track.bypass_dynamics"),
            (Key::V, shift, "track.bypass_reverb"),
            (Key::D, shift, "track.bypass_delay"),
            (Key::W, shift, "track.bypass_modulation"),
            (Key::Q, shift, "track.mute_sends"),
            (Key::Num4, shift, "track.mute_sends_ae"),
            (Key::Num5, shift, "track.mute_sends_fj"),
            (Key::U, alt_shift, "edit.tce_to_timeline"),
            (Key::CloseBracket, alt, "edit.play_timeline"),
            (Key::Questionmark, Modifiers { alt: true, shift: true, ..cmd }, "edit.automation_trim_to_all"),
            (Key::Questionmark, alt_shift, "edit.automation_glide_to_all"),
            (Key::Slash, alt_shift, "edit.automation_glide_to_all"),
            (Key::Questionmark, shift, "options.link_timeline_edit"),
            (Key::J, Modifiers { shift: true, ..cmd }, "video.online"),
            (Key::V, alt, "edit.paste_merge_markers"),
            (Key::M, alt, "edit.paste_merge_midi"),
            (Key::X, alt_shift, "edit.cut_all_automation"),
        ] {
            let before = app.engine.journal.len();
            press(&ctx, &mut app, key, mods);
            assert_eq!(app.engine.journal.len(), before + 1, "{os:?} {key:?} {mods:?}: {}", app.ui.status);
            assert_eq!(app.engine.journal.last().unwrap().0, id, "{os:?} {key:?} {mods:?}");
        }
        press(&ctx, &mut app, Key::P, alt_shift);
        assert_eq!(app.dialogs.open_name(), Some("path"), "{os:?} Save Track Preset");
    }
}

#[test]
fn ctrl_shift_arrows_nudge_the_selected_clip_gain() {
    let pc_ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
    for (os, ctrl) in [(OperatingSystem::Mac, Modifiers::CTRL), (OperatingSystem::Windows, pc_ctrl), (OperatingSystem::Nix, pc_ctrl)] {
        let mut app = SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default());
        let kick = app.engine.session().track_by_name("Kick").unwrap().clips()[0].id;
        app.engine.execute("edit.select", &json!({"clips": [kick.0]})).unwrap();
        let ctx = egui::Context::default();
        ctx.set_os(os);
        let ctrl_shift = Modifiers { shift: true, ..ctrl };
        press(&ctx, &mut app, Key::ArrowUp, ctrl_shift);
        press(&ctx, &mut app, Key::ArrowUp, ctrl_shift);
        press(&ctx, &mut app, Key::ArrowDown, ctrl_shift);
        assert_eq!(app.engine.session().find_clip(kick).unwrap().1.gain_db, 1.0, "{os:?}");
    }
}
