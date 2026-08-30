fn main() {
    let code = match dev_cli::run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("错误: {error:#}");
            1
        }
    };
    std::process::exit(code);
}
