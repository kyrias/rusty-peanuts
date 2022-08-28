pub mod api;
pub mod html;

pub(super) fn mount(app: &mut tide::Server<crate::State>) {
    html::mount(app);
    api::mount(app.at("/api"));
}
