use std::io::IsTerminal;

fn main() {
    println!("{}", std::io::stdout().is_terminal());
}
