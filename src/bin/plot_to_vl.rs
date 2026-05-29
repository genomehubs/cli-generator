use std::io::{self, Read};

fn main() {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("failed to read stdin: {}", e);
        std::process::exit(2);
    }

    let out = genomehubs_query::plot_spec_to_vega_lite_json(&input);
    println!("{}", out);
}
