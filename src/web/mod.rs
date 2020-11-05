pub mod api;
pub mod html;

pub(super) fn mount(mut app: &mut tide::Server<crate::State>) {
    html::mount(&mut app);
    api::mount(app.at("/api"));
}
