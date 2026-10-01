//! Minimal example: load a .litertlm model and ask a question.
//! Usage: cargo run --example ask -- <model.litertlm> ["question"]
use litert_rs::LiteRtModel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let model_path = args.get(1).expect("usage: ask <model.litertlm> [question]");
    let prompt = args
        .get(2)
        .map(|s| s.as_str())
        .unwrap_or("What is 2 + 2? Answer briefly.");

    println!("Loading {} (CPU)…", model_path);
    let model = LiteRtModel::load(model_path, "cpu", 2048)?;
    println!("Q: {prompt}");
    let answer = model.ask(prompt)?;
    println!("A: {answer}");
    Ok(())
}
