use sqlx::migrate::Migrator;

pub static MIGRATOR: Migrator = sqlx::migrate!();

pub fn get_migrator() -> &'static Migrator {
    &MIGRATOR
}
