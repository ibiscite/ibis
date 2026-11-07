# 0024 Window permissions

Every Ibis command is declared in Tauri's app manifest and granted per window;
remote origins get none. A test checks the grants through Tauri's IPC
dispatcher.
