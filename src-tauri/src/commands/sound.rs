use crate::services::sound::{SoundPlayer, AppSounds};

#[tauri::command]
pub fn play_copy_sound() {
    AppSounds::play_copy();
}

#[tauri::command]
pub fn play_paste_sound() {
    AppSounds::play_paste();
}

#[tauri::command]
pub fn play_scroll_sound() {
    AppSounds::play_scroll();
}

