use log::{LevelFilter, SetLoggerError};
use systemd_journal_logger::JournalLog;

pub fn setup(level: LevelFilter) -> Result<(), SetLoggerError> {
    JournalLog::default().install()?;
    log::set_max_level(level);

    Ok(())
}
