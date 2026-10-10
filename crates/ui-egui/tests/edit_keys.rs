use egui::os::OperatingSystem;
use egui::{Event, Modifiers, OutputCommand};
use serde_json::json;
use soundcraft_ui_egui::{Services, SoundApp, shortcuts};

const MAC_CMD: Modifiers = Modifiers { mac_cmd: true, command: true, ..Modifiers::NONE };

fn send(ctx: &egui::Context, app: &mut SoundApp, event: Event, modifiers: Modifiers) -> Vec<OutputCommand> {
    let input = egui::RawInput { events: vec![Event::ModifiersChanged(modifiers), event], ..Default::default() };
    let mut out = ctx.run_ui(input, |ui| {
        app.logic(ui.ctx());
        shortcuts::handle(app, ui.ctx());
    });
    out.textures_delta.clear();
    out.platform_output.commands
}

fn demo() -> SoundApp {
    SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default())
}

fn kick_clips(app: &SoundApp) -> usize {
    app.engine.session().track_by_name("Kick").unwrap().clips().len()
}

fn select_first_kick_clip(app: &mut SoundApp) {
    let first = app.engine.session().track_by_name("Kick").unwrap().clips()[0].id.0;
    app.run("edit.select", json!({"clips": [first]})).unwrap();
}

#[test]
fn the_desktop_clipboard_keys_cut_copy_and_paste_clips() {
    let pc_ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
    for (os, cmd) in [(OperatingSystem::Mac, MAC_CMD), (OperatingSystem::Windows, Modifiers::SHIFT), (OperatingSystem::Nix, pc_ctrl)] {
        for text in [false, true] {
            let (mut app, ctx) = (demo(), egui::Context::default());
            app.services.clipboard_has_text = Some(Box::new(move || text));
            ctx.set_os(os);
            let clips = kick_clips(&app);

            let placeholder =
                |app: &mut SoundApp| send(&ctx, app, Event::PointerGone, Modifiers::NONE).iter().any(|c| matches!(c, OutputCommand::CopyText(_)));
            select_first_kick_clip(&mut app);
            app.run("edit.copy_clip_gain", json!({})).unwrap();
            assert_eq!(placeholder(&mut app), !text, "Copy Special, {os:?}, text on the clipboard: {text}");
            send(&ctx, &mut app, Event::Copy, cmd);
            assert_eq!(placeholder(&mut app), !text, "{os:?}, text on the clipboard: {text}");

            app.run("edit.select", json!({"tracks": ["Kick"], "start": 0})).unwrap();
            send(&ctx, &mut app, Event::Paste("SoundCraft".into()), cmd);
            assert_eq!(kick_clips(&app), clips + 1, "{os:?}");

            select_first_kick_clip(&mut app);
            send(&ctx, &mut app, Event::Cut, cmd);
            assert_eq!(kick_clips(&app), clips, "{os:?}");
        }
    }
}
