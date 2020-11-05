#[macro_use]
pub mod utils;
pub mod v1;

pub(super) fn mount(mut route: tide::Route<crate::State>) {
    v1::mount(route.at("/v1"));
}
