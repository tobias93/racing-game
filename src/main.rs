pub mod renderer;

use renderer::Renderer;
use std::time::Duration;

fn main() {
    dotenv::dotenv().ok();
    pretty_env_logger::init();

    let r = Renderer::new().unwrap();
    r.run_event_loop();
}
