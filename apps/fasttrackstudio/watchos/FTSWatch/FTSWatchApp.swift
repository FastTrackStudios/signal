// FastTrackStudio watch remote — entry point. A TabView: the perform grid
// (the app) and settings (transport + engine host).

import SwiftUI

@main
struct FTSWatchApp: App {
    @State private var store = RigStore()

    var body: some Scene {
        WindowGroup {
            TabView {
                PerformGridView()
                    .ignoresSafeArea(edges: .bottom)
                SettingsView()
            }
            .tabViewStyle(.verticalPage)
            .environment(store)
        }
    }
}
