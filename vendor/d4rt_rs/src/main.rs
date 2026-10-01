fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        println!("d4rt_rs - Dart interpreter (in progress)");
        println!("Usage: d4rt_rs <file.dart>");
        return;
    }
    match std::fs::read_to_string(&args[0]) {
        Ok(source) => {
            d4rt_rs::stdlib::set_main_script_path(&args[0]);
            // The CLI is a trusted, explicit invocation: everything is
            // allowed, including loading import/export modules from disk.
            let mut ctx = d4rt_rs::Context::new_with(d4rt_rs::SandboxOptions::allow_all());
            match ctx.eval(&source) {
                Ok(_) => {}
                Err(d4rt_rs::InterpError::Throw(v)) => {
                    eprintln!("Uncaught exception: {}", d4rt_rs::value::value_display(&v));
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Cannot read file {}: {e}", args[0]);
            std::process::exit(1);
        }
    }
}
