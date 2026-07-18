import Foundation

public struct NodeApiError: Error, CustomStringConvertible {
    public let code: Int
    public let bodyText: String
    public var description: String { "HTTP \(code): \(bodyText)" }
}

/// Thin HTTP client for the node's JSON API - mirrors NodeApi.kt exactly.
/// No retries, no caching: build the JSON in HazeWalletCore's business
/// logic (via the Rust FFI), hand it here to move across the wire.
public actor NodeApi {
    private var baseUrl: String
    private let session = URLSession(configuration: .default)

    public init(baseUrl: String) {
        self.baseUrl = baseUrl.hasSuffix("/") ? String(baseUrl.dropLast()) : baseUrl
    }

    public func setBaseUrl(_ url: String) {
        baseUrl = url.hasSuffix("/") ? String(url.dropLast()) : url
    }

    public func currentBaseUrl() -> String { baseUrl }

    private func get(_ path: String) async throws -> Data {
        var request = URLRequest(url: URL(string: baseUrl + path)!)
        request.httpMethod = "GET"
        let (data, response) = try await session.data(for: request)
        try Self.checkStatus(response, data)
        return data
    }

    private func post(_ path: String, jsonBody: String) async throws -> Data {
        var request = URLRequest(url: URL(string: baseUrl + path)!)
        request.httpMethod = "POST"
        request.setValue("application/json; charset=utf-8", forHTTPHeaderField: "Content-Type")
        request.httpBody = jsonBody.data(using: .utf8)
        let (data, response) = try await session.data(for: request)
        try Self.checkStatus(response, data)
        return data
    }

    private static func checkStatus(_ response: URLResponse, _ data: Data) throws {
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            let code = (response as? HTTPURLResponse)?.statusCode ?? -1
            throw NodeApiError(code: code, bodyText: String(data: data, encoding: .utf8) ?? "")
        }
    }

    /// GET /v1/utxos - every commitment in the UTXO set, as lowercase hex.
    public func utxos() async throws -> [String] {
        let data = try await get("/v1/utxos")
        let arrays = try JSONDecoder().decode([[UInt8]].self, from: data)
        return arrays.map { bytes in bytes.map { String(format: "%02x", $0) }.joined() }
    }

    public func status() async throws -> [String: Any] {
        let data = try await get("/v1/status")
        return (try JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:]
    }

    public func feeEstimate() async throws -> [String: Any] {
        let data = try await get("/v1/fee-estimate")
        return (try JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:]
    }

    /// GET /v1/scan-outputs - raw JSON array, passed straight through to
    /// the Rust FFI for note-recovery scanning.
    public func scanOutputsJson() async throws -> String {
        let data = try await get("/v1/scan-outputs")
        return String(data: data, encoding: .utf8) ?? "[]"
    }

    public func submitTransaction(_ transactionJson: String) async throws {
        _ = try await post("/v1/transactions", jsonBody: transactionJson)
    }

    public func submitStake(_ stakeRequestJson: String) async throws {
        _ = try await post("/v1/stake", jsonBody: stakeRequestJson)
    }

    /// POST /v1/faucet -> slate_json
    public func requestFaucet(amount: UInt64) async throws -> String {
        let body = try JSONSerialization.data(withJSONObject: ["amount": amount])
        let data = try await post("/v1/faucet", jsonBody: String(data: body, encoding: .utf8)!)
        let obj = try JSONSerialization.jsonObject(with: data) as! [String: Any]
        return obj["slate_json"] as! String
    }

    public func completeFaucet(responseSlateJson: String) async throws {
        let body = try JSONSerialization.data(withJSONObject: ["response_slate_json": responseSlateJson])
        _ = try await post("/v1/faucet/complete", jsonBody: String(data: body, encoding: .utf8)!)
    }

    /// GET /v1/names/:name -> nil if not registered.
    public func resolveName(_ name: String) async throws -> [String: Any]? {
        do {
            let data = try await get("/v1/names/\(name)")
            return try JSONSerialization.jsonObject(with: data) as? [String: Any]
        } catch let err as NodeApiError where err.code == 404 {
            return nil
        }
    }

    public func registerName(opJson: String) async throws {
        _ = try await post("/v1/names/register", jsonBody: opJson)
    }

    public func registerNameSponsored(reqJson: String) async throws {
        _ = try await post("/v1/names/register-sponsored", jsonBody: reqJson)
    }

    public func transferName(opJson: String) async throws {
        _ = try await post("/v1/names/transfer", jsonBody: opJson)
    }

    public func postInbox(pubkeyHex: String, fromPubkeyHex: String, kind: String, payloadJson: String) async throws {
        let body = try JSONSerialization.data(withJSONObject: [
            "from_pubkey_hex": fromPubkeyHex,
            "kind": kind,
            "payload_json": payloadJson,
        ])
        _ = try await post("/v1/inbox/\(pubkeyHex)", jsonBody: String(data: body, encoding: .utf8)!)
    }

    /// GET /v1/inbox/:pubkeyHex - drains and returns queued messages.
    public func getInbox(pubkeyHex: String) async throws -> [[String: Any]] {
        let data = try await get("/v1/inbox/\(pubkeyHex)")
        return (try JSONSerialization.jsonObject(with: data) as? [[String: Any]]) ?? []
    }
}
