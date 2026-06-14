use rusqlite::Connection;
use tokio::sync::Mutex;

pub struct AppState {
    pub conn: Mutex<Connection>,
}
