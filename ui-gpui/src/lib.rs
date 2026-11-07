// Winkel's GPUI frontend: the same std-only backend on the same wire, a
// window the same size with the same words. The root binary calls run()
// for the window path and --backend for the metronome.

pub mod app;
mod font;
mod hero;
pub mod i18n;
mod json;
mod layout;
mod note_figure;
mod play_mark;
pub mod protocol;
mod render;
pub mod rhythm;
mod text;
pub mod theme;
#[cfg(test)]
mod tests;
mod wheel;

use gpui::prelude::*;
use gpui::{px, size, App, Application, Bounds, WindowBounds, WindowOptions};

/// Open the window and drive the backend child at `backend_bin`.
/// Does not return until the app quits.
pub fn run(backend_bin: String) {
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(
            None,
            size(px(app::WINDOW_W as f32), px(app::WINDOW_H as f32)),
            cx,
        );
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("dev.douglasmoura.winkel".into()),
            window_min_size: Some(size(px(app::WINDOW_W as f32), px(app::WINDOW_H as f32))),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            let entity = cx.new(|cx| {
                let focus = cx.focus_handle();
                focus.focus(window);
                let hero_focus = cx.focus_handle();
                let mut view = app::AppView::new(focus, hero_focus, backend_bin.clone());

                match protocol::Bridge::spawn(&backend_bin) {
                    Ok((bridge, events)) => {
                        view.bridge = Some(bridge);
                        if let Some(b) = &view.bridge {
                            b.send(&protocol::Request::Hello);
                        }
                        let pump = cx.spawn(async move |this, cx| {
                            while let Ok(event) = events.recv().await {
                                let continue_ = this.update(cx, |view: &mut app::AppView,
                                                               cx: &mut Context<app::AppView>| {
                                    view.handle_event(event, cx)
                                });
                                if !continue_.unwrap_or(false) {
                                    let _ = this.update(cx, |_, cx| cx.quit());
                                    break;
                                }
                            }
                        });
                        view.tasks.push(pump);
                    }
                    Err(message) => {
                        view.failure = message;
                    }
                }

                let watcher = cx.spawn(async move |this, cx| {
                    let mut last = this
                        .update(cx, |v, _| v.theme.theme_name_mtime())
                        .ok()
                        .flatten();
                    loop {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(500))
                            .await;
                        let _ = this.update(cx, |view, cx| {
                            let current = view.theme.theme_name_mtime();
                            if current != last {
                                last = current;
                                view.theme.reload();
                                view.texts.invalidate();
                                cx.notify();
                            }
                        });
                    }
                });
                view.tasks.push(watcher);

                view
            });

            {
                let entity = entity.clone();
                window.on_window_should_close(cx, move |_window, cx| {
                    let _ = entity.update(cx, |view, cx| view.request_quit(cx));
                    false
                });
            }

            entity
        })
        .unwrap();
        cx.activate(true);

        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
}
