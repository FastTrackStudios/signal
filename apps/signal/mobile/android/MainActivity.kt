package dev.dioxus.main

// Blitz draws into Android's NativeActivity; the Rust side's entry is
// `android_main` (src/main.rs), loaded from lib<lib_name>.so.
class MainActivity : android.app.NativeActivity()
