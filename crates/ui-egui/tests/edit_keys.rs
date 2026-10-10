use egui::os::OperatingSystem;
use egui::{Event, Key, Modifiers, OutputCommand};
use serde_json::json;
use soundcraft_ui_egui::{MainWindow, Services, SoundApp, shortcuts};

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

#[test]
fn delete_and_backspace_clear_the_selected_clip_unless_midi_notes_take_them() {
    for key in [Key::Delete, Key::Backspace] {
        for (window, editor, notes, cleared) in [
            (MainWindow::Edit, false, vec![], true),
            (MainWindow::Edit, false, vec![0], true),
            (MainWindow::Edit, true, vec![], true),
            (MainWindow::Edit, true, vec![0], false),
            (MainWindow::Mix, false, vec![], false),
        ] {
            let mut app = demo();
            let clips = kick_clips(&app);
            select_first_kick_clip(&mut app);
            (app.ui.window, app.ui.show_midi_editor, app.midi.selected) = (window, editor, notes);
            let event = Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
            send(&egui::Context::default(), &mut app, event, Modifiers::NONE);
            assert_eq!(kick_clips(&app), clips - usize::from(cleared), "{key:?} {window:?}, editor open: {editor}, notes: {:?}", app.midi.selected);
        }
    }
}
