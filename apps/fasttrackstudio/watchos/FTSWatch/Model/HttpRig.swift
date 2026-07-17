// The engine transport: JSON over the `/watch/v1` HTTP+SSE bridge served
// by `fasttrackstudio --engine` (default port 4040). Plain URLSession only —
// watchOS forbids WebSockets outside audio-streaming sessions (TN3135), so
// vox's `/vox` endpoint is out of reach; the bridge is an in-process vox
// client on the engine instead. State arrives over a server-sent-events
// stream (one `data:` line per WatchState); commands are bare POSTs.

import Foundation

@MainActor
final class HttpRig: RigTransport {
    var onState: ((WatchState) -> Void)?
    var onConnected: ((Bool) -> Void)?

    private let base: URL
    private var streamTask: Task<Void, Never>?
    private let session: URLSession

    /// `host` like "192.168.1.20:4040" (scheme optional).
    init?(host: String) {
        let trimmed = host.trimmingCharacters(in: .whitespaces)
        let urlString = trimmed.contains("://") ? trimmed : "http://\(trimmed)"
        guard let url = URL(string: urlString), url.host() != nil else { return nil }
        self.base = url
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 10
        // The SSE stream is long-lived; don't let the resource timeout kill it.
        config.timeoutIntervalForResource = .infinity
        config.waitsForConnectivity = true
        self.session = URLSession(configuration: config)
    }

    func start() {
        streamTask?.cancel()
        streamTask = Task { [weak self] in
            await self?.streamLoop()
        }
    }

    func stop() {
        streamTask?.cancel()
        streamTask = nil
    }

    func pressStack(_ index: Int) {
        post("watch/v1/press/\(index)")
    }

    func perform(_ action: RigAction) {
        post("watch/v1/action/\(action.rawValue)")
    }

    private func post(_ path: String) {
        var request = URLRequest(url: base.appending(path: path))
        request.httpMethod = "POST"
        session.dataTask(with: request).resume()
    }

    /// Consume `/watch/v1/events`, reconnecting with a short backoff. Each
    /// SSE `data:` line is a complete WatchState JSON document.
    private func streamLoop() async {
        let decoder = JSONDecoder()
        while !Task.isCancelled {
            var request = URLRequest(url: base.appending(path: "watch/v1/events"))
            request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
            do {
                let (bytes, response) = try await session.bytes(for: request)
                guard (response as? HTTPURLResponse)?.statusCode == 200 else {
                    throw URLError(.badServerResponse)
                }
                onConnected?(true)
                for try await line in bytes.lines {
                    guard line.hasPrefix("data:") else { continue }
                    let json = line.dropFirst(5).trimmingCharacters(in: .whitespaces)
                    guard let data = json.data(using: .utf8),
                        let state = try? decoder.decode(WatchState.self, from: data)
                    else { continue }
                    onState?(state)
                }
            } catch {
                // fall through to reconnect
            }
            onConnected?(false)
            try? await Task.sleep(for: .seconds(2))
        }
    }
}
