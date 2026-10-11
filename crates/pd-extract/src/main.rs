mod gfx;
mod scene;
mod world;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    let result = match args.first().map(String::as_str) {
        Some("gfx") => gfx::run(rest),
        Some("world") => world::run(rest),
        Some("scene") => scene::run(rest),
        _ => Err("usage: pd-extract <gfx|world> [args...]".into()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
