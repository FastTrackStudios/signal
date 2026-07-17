// Transport selection + engine host. Reached by swiping to the second tab.

import SwiftUI

struct SettingsView: View {
    @Environment(RigStore.self) private var store

    var body: some View {
        @Bindable var store = store
        Form {
            Section("Transport") {
                Picker("Mode", selection: $store.transportKind) {
                    ForEach(TransportKind.allCases) { kind in
                        Text(kind.rawValue).tag(kind)
                    }
                }
            }
            if store.transportKind == .engine {
                Section("Engine") {
                    TextField("host:port", text: $store.engineHost)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .onSubmit { store.reconnect() }
                    Button("Reconnect") { store.reconnect() }
                }
            }
            Section {
                LabeledContent("Status", value: store.connected ? "Connected" : "Offline")
                LabeledContent("Profile", value: store.state.profileName)
            }
        }
    }
}
