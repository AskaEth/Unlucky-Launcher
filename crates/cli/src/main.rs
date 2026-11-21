//! `ull` 二进制入口：把真实 stdio 交给 [`unlucky_cli::run`]，退出码照传。

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    let code = unlucky_cli::run(&args, &mut stdout, &mut stderr);
    std::process::exit(code);
}
