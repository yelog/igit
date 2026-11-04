pub mod commit;
pub mod core;
pub mod log;
pub mod merge;
pub mod push;
mod shell;
pub mod ui;
pub fn run() -> anyhow::Result<()> {
    shell::run()
}
