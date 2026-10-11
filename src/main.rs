mod backend;
mod design;
mod icons;
mod ui;
fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--inventory") {
        match backend::inventory() {
            Ok(items) => println!("{}", serde_json::to_string_pretty(&items).unwrap()),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--prepare") {
        let path = args.get(2).expect("Usage: dropdock --prepare PATH");
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            for event in rx {
                if let backend::Event::Log(s) = event {
                    println!("{s}");
                }
            }
        });
        let result = backend::operation_lock().and_then(|_guard| {
            backend::inspect(path.into()).and_then(|p| backend::prepare(p, &tx))
        });
        drop(tx);
        worker.join().ok();
        match result {
            Ok(packages) => {
                for p in packages {
                    println!("Prepared: {}", p.path.display());
                }
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1)
            }
        }
        return Ok(());
    }
    let icon = image::load_from_memory(include_bytes!("../dropdock.png"))
        .expect("Bundled app icon")
        .into_rgba8();
    let (width, height) = icon.dimensions();
    let icon = eframe::egui::IconData {
        rgba: icon.into_raw(),
        width,
        height,
    };
    let size = std::env::var("DROPDOCK_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
        })
        .unwrap_or([1180.0, 820.0]);
    eframe::run_native(
        "Dropdock",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size(size)
                .with_min_inner_size([860.0, 640.0])
                .with_app_id("io.dropdock.App")
                .with_icon(icon),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(ui::Dropdock::new(cc)))),
    )
}
