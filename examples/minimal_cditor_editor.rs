use CDitor_V2::Cditor;
use gpui::*;

fn main() {
    let app = gpui_platform::application();

    app.run(move |cx: &mut App| {
        cx.activate(true);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: Size {
                        width: px(960.0),
                        height: px(640.0),
                    },
                })),
                titlebar: Some(TitlebarOptions {
                    title: Some("Minimal CDitor".into()),
                    appears_transparent: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_window, cx| {
                cx.new(|cx| {
                    Cditor::new()
                        .demo()
                        .with_debug_overlay(false)
                        .with_payload_window_size(256)
                        .build_view(cx)
                })
            },
        )
        .expect("failed to open minimal CDitor window");
    });
}
