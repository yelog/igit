use super::*;
use crate::log::file_diff::FileDiff;
use std::sync::mpsc;

pub(super) fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    workspace: PathBuf,
    file: PathBuf,
    line: usize,
    buffer: Option<PathBuf>,
    icons: IconMode,
) -> Result<()> {
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("igit-file-diff".into())
        .spawn(move || {
            let result = Context::discover(&workspace)
                .and_then(|context| FileDiff::open(&context, &file, line, buffer.as_deref()));
            let _ = sender.send(result);
        })?;
    let mut panel = loop {
        terminal.draw(|frame| {
            frame.render_widget(
                Paragraph::new("Loading File Diff...  Esc / q Close")
                    .style(Style::new().bg(BG).fg(FG)),
                frame.area(),
            );
        })?;
        match receiver.try_recv() {
            Ok(result) => break result?,
            Err(mpsc::TryRecvError::Disconnected) => {
                bail!("File Diff loader stopped unexpectedly")
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if event::poll(Duration::from_millis(80))? && startup::cancelled(&event::read()?) {
            return Ok(());
        }
    };
    loop {
        terminal.draw(|frame| panel.draw(frame, frame.area(), icons))?;
        let input = event::read()?;
        if panel.event(&input) {
            return Ok(());
        }
    }
}
