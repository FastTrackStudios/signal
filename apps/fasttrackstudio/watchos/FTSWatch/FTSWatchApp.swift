// FastTrackStudio watch remote — entry point. A TabView: the perform grid
// (the app) and settings (transport + engine host).

import SwiftUI

@main
struct FTSWatchApp: App {
    @State private var store = RigStore()
    // Initial tab override for demo/screenshot automation:
    // SIMCTL_CHILD_FTS_TAB=perform|chords|session|settings.
    @State private var tab: String =
        ProcessInfo.processInfo.environment["FTS_TAB"] ?? "perform"

    var body: some Scene {
        WindowGroup {
            TabView(selection: $tab) {
                PerformGridView()
                    .tag("perform")
                ChordsView()
                    .tag("chords")
                SessionView()
                    .tag("session")
                SettingsView()
                    .tag("settings")
            }
            .tabViewStyle(.verticalPage)
            .environment(store)
        }
    }
}
