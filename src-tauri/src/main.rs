#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod chrome;
mod commands;
mod db;
mod docker;
mod model;
mod store;
mod tray;

use commands::AppState;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let repo = store::Repo::new(dir);
            repo.init();
            app.manage(AppState { registry: db::Registry::default(), repo });
            tray::install(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::profile_list,
            commands::profile_save,
            commands::profile_delete,
            commands::history_list,
            commands::history_clear,
            commands::conn_connect,
            commands::conn_test,
            commands::conn_close,
            commands::conn_open,
            commands::meta_databases,
            commands::meta_schemas,
            commands::meta_tables,
            commands::meta_table,
            commands::grid_page,
            commands::script_run,
            commands::edits_apply,
            commands::export_csv,
            commands::ddl_create_table,
            commands::ddl_drop,
            commands::ddl_truncate,
            commands::map_schema,
            commands::overview,
            commands::docker_inspect,
            commands::chrome_apply,
            commands::window_title
        ])
        .run(tauri::generate_context!())
        .expect("OrangeData 启动失败");
}
