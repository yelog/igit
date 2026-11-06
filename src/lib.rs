pub mod branch;
pub mod commit;
mod config;
pub mod core;
mod installation;
pub mod log;
pub mod merge;
pub mod push;
mod shell;
pub mod stash;
pub mod ui;
pub fn run() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    if args.next().is_some_and(|argument| argument == "update") {
        return installation::run(args);
    }
    shell::run()
}
