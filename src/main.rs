mod renderer;

use crate::renderer::main_loop::{main_loop, App, DrawContext, RenderLoopSettings};
use log::info;

pub fn main() {
    dotenv::dotenv().ok();
    pretty_env_logger::init();

    let settings = RenderLoopSettings::default();
    let app = TestApp;
    main_loop(settings, app);
}

struct TestApp;

impl App for TestApp {
    fn draw(&mut self, context: &mut DrawContext) {
        println!("Frame!")
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        println!("Bye!")
    }
}
