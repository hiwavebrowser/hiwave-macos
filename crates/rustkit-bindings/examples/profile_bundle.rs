//! Child of scripts/profile_bundle.py. The parent installs its limits before sending a byte.
use rustkit_bindings::DomBindings;
use rustkit_dom::Document;
use rustkit_js::JsRuntime;
use std::{env, error::Error, fs, io::Read, rc::Rc};
use url::Url;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn profile() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let bundle = fs::read_to_string(args.next().ok_or("expected a JavaScript file")?)?;
    let url = Url::parse(&args.next().ok_or("expected a page URL")?.to_string_lossy())?;
    let html = match args.next() {
        Some(file) => fs::read_to_string(file)?,
        None => "<!doctype html><html><head></head><body></body></html>".to_owned(),
    };
    let bindings = DomBindings::new(JsRuntime::new()?)?;
    bindings.set_document(Rc::new(Document::parse_html(&html)?))?;
    bindings.set_location(&url)?;
    bindings.enable_net_bridge()?;
    let name = url.join("profile-bundle.js")?;
    bindings.register_script(name.as_str(), url.as_str(), false);
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter("rustkit_js::script_phases=debug")
        .without_time()
        .with_ansi(false)
        .finish();
    let outcome = tracing::subscriber::with_default(subscriber, || {
        bindings.evaluate_named(&bundle, name.as_str())
    });
    match outcome {
        Ok(_) => println!(
            "BUNDLE_RESULT {}",
            serde_json::json!({"status":"completed"})
        ),
        Err(e) => println!(
            "BUNDLE_RESULT {}",
            serde_json::json!({"status":"threw","error":e.to_string().chars().take(400).collect::<String>()})
        ),
    }
    Ok(())
}

fn main() -> Result<()> {
    std::io::stdin().read_exact(&mut [0; 1])?;
    const STACK: usize = 16 * 1024 * 1024;
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(|| {
            rustkit_js::set_thread_native_stack_budget(Some(STACK / 2));
            profile()
        })?
        .join()
        .map_err(|_| "profile thread panicked")?
}
