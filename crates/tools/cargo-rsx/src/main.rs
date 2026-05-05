mod runner;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("rsx") {
        runner::run(args.into_iter().skip(1).collect());
    } else {
        runner::run(args);
    }
}
